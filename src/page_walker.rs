use crate::{
    request_maker::*,
    transform_html::{self, defs::*, *},
};
use anyhow::Error;
use derive_more::Display;
use thiserror::Error;
use tracing::info;

use super::etl_config_parser::*;
use serde_yaml;
use std::{
    borrow::{Borrow, BorrowMut, Cow},
    cell::RefCell,
    fmt::Debug,
    fs::{self, File},
    io::BufReader,
    path::{self, Path, PathBuf},
    rc::{Rc, Weak},
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Debug)]
pub struct PageWalker {
    source_name: String,
    etl_config_path: PathBuf,
    etl_config: EtlConfig,
    source_config_idx: usize,
    request_maker: RequestMaker,
    menu_page_url_sub: String,
    max_depth_level: usize,
}

type ConsumerType = dashmap::DashMap<usize, Vec<DataMap>>;

#[derive(Error, Debug)]
pub enum PageWalkerError {
    #[error("couldn't open {0}")]
    FileError(#[from] std::io::Error),
    #[error("couldn't parse {0}")]
    SerdeYamlError(#[from] serde_yaml::Error),
    #[error("couldn't parse config {0}")]
    ParseConfigError(PathBuf, #[source] Box<PageWalkerError>),
    #[error("couldn't find source by name {0}")]
    SourceConfigNotFound(String),
    #[error("couldn't create or use RequestMaker")]
    RequestMakerError(#[from] RequestMakerError),
    #[error("couldn't create or use RequestMaker")]
    TransformHtmlError(#[from] TransformError),
}

impl PageWalker {
    fn create_inner(
        source_name: String,
        etl_config_path: &PathBuf,
        max_depth_level: usize,
    ) -> Result<PageWalker, PageWalkerError> {
        let etl_config_path = etl_config_path.to_owned();
        let etl_config = PageWalker::parse_config(etl_config_path.as_path())?;
        let source_config_idx = PageWalker::extract_source_config(&etl_config, &source_name)?;
        let menu_page_url_sub = transform_html::defs::prepare_rx_sub_for_replace(
            etl_config.sources[source_config_idx]
                .menu
                .page_url_sub
                .as_str(),
        );
        let retries = &etl_config.http.retries;
        let request_maker = RequestMaker::create(RequestMakerConfig {
            headers: etl_config.http.headers.clone(),

            backoff_factor: retries.backoff_factor,
            max_retries: retries.max_retries,
            timeout: Duration::from_secs(retries.timeout.into()),
            status_forcelist: retries.status_forcelist.clone(),
        })?;

        Ok(Self {
            etl_config_path,
            source_name,
            etl_config,
            source_config_idx,
            menu_page_url_sub,
            request_maker,
            max_depth_level,
        })
    }

    pub fn create(
        source_name: String,
        etl_config_path: &PathBuf,
        max_depth_level: usize,
    ) -> Result<PageWalker, PageWalkerError> {
        let abs_path = std::path::absolute(etl_config_path).unwrap();

        PageWalker::create_inner(source_name, etl_config_path, max_depth_level)
            .map_err(|err| PageWalkerError::ParseConfigError(dbg!(abs_path), err.into()))
    }

    pub fn parse_config(etl_config_path: &Path) -> Result<EtlConfig, PageWalkerError> {
        let file = File::open(etl_config_path)?;
        let etl_config: EtlConfig = serde_yaml::from_reader(BufReader::new(file))?;
        Ok(etl_config)
    }

    pub fn extract_source_config(
        etl_config: &EtlConfig,
        source_name: &String,
    ) -> Result<usize, PageWalkerError> {
        if let Some((idx, _)) = etl_config
            .sources
            .iter()
            .enumerate()
            .find(|(idx, s)| s.name == *source_name)
        {
            return Ok(idx);
        }
        Err(PageWalkerError::SourceConfigNotFound(
            source_name.to_owned(),
        ))
    }

    pub fn source_config(&self) -> &SourceConfig {
        &self.etl_config.sources[self.source_config_idx]
    }

    fn sub_page_number(&self, filter_url: &str, page_number: usize) -> String {
        if page_number < 2 {
            return url_combine_all(&[
                &self.source_config().root_url,
                filter_url,
                &self.source_config().menu.first_page_url,
            ]);
        }
        let page_url = regex::Regex::new(r"(\d+)")
            .unwrap()
            .replace_all(page_number.to_string().as_str(), &self.menu_page_url_sub)
            .into_owned();
        return url_combine_all(&[&self.source_config().root_url, filter_url, &page_url]);
    }

    async fn extract_data(
        &self,
        url: &str,
        rules: &Vec<ParserTransfromRule>,
    ) -> Result<DataMap, PageWalkerError> {
        info!("read page [{url}]");
        let response = self
            .request_maker
            .request_text(&RequestParams {
                method: "GET".to_owned(),
                url: url.to_owned(),
                ..Default::default()
            })
            .await?;
        let data = transform_html(
            &response,
            rules,
            &TransformSettings {
                ..Default::default()
            },
        )?;
        Ok(data)
    }

    async fn parse_menu_page(
        &self,
        filter_url: &str,
        page_number: usize,
    ) -> Result<DataMap, PageWalkerError> {
        let url = self.sub_page_number(filter_url, page_number);
        self.extract_data(&url, &self.source_config().menu.rules)
            .await
    }

    async fn parse_card_page(&self, url_part: &str) -> Result<DataMap, PageWalkerError> {
        let url = url_combine(&self.source_config().root_url, &url_part);
        self.extract_data(&url, &self.source_config().card.rules)
            .await
    }

    pub async fn walk_on_menu_page(
        &self,
        filter_url: &str,
        num: usize,
        consumer: &ConsumerType,
    ) -> Result<(), PageWalkerError> {
        let menu = self.parse_menu_page(filter_url, num).await?;
        let menu_items = menu["menu_items"].exract_list();

        if menu_items.is_empty() {
            consumer.insert(num, Vec::new());
        }

        let mut card_data_list = Vec::new();

        for ele in menu_items.iter() {
            let url = ele
                .exract_dict()
                .get("url")
                .expect("couldn't found 'url'")
                .exract_value();
            let card_data = self.parse_card_page(url).await?;
            card_data_list.push(card_data);
        }
        consumer.insert(num, card_data_list);
        Ok(())
    }
}

pub async fn walk(
    walker: &PageWalker,
    filter_url: &str,
    begin: usize,
    end: usize,
    consumer: &ConsumerType,
) {
    let mut result_group = Vec::with_capacity(end - begin + 2);

    for num in begin..end + 1 {
        // let slf = self_ref.clone().get_mut();
        result_group.push(walker.walk_on_menu_page(filter_url, num, consumer));
    }
    for ele in result_group {
        ele.await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iter() {
        let mut vars = (1, 11, false);
        for num in vars.0..vars.1 + 1 {
            vars.2 |= (vars.1 == num && num != vars.1 + 1)
        }
        assert_eq!(vars.2, true);
    }
}
