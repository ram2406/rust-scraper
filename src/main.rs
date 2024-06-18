use std::path::PathBuf;
use std::str;
use std::vec::Vec;
use std::env;
use tokio;

use clap::Parser;
mod etl_config_parser;
mod page_walker;
mod request_maker;
mod transform_html;

use page_walker::PageWalker;

use tracing::{info, Level};

use crate::page_walker::walk;

/// Simple program to greet a person
#[derive(Parser, Debug, PartialEq)]
#[command(version, about, long_about = None)]
struct Args {
    /// Etl config file location
    #[arg(short = 'p', long)]
    etl_config_path: PathBuf,

    /// Source name from config file
    #[arg(short, long)]
    source_name: String,

    /// Filter url pattern
    #[arg(short, long)]
    filter_url: String,

    /// Output file with JSON format
    #[arg(short, long, default_value = "./output.json")]
    output_file_path: PathBuf,

    /// Scraping will start from begin page
    #[arg(short, long, default_value_t = 1)]
    begin_page: usize,

    /// Scraping will stop on end page
    #[arg(short, long, default_value_t = 1)]
    end_page: usize,

    /// Html parser max depth limit
    #[arg(short = 'l', long, default_value_t = 10_000)]
    rule_max_depth_limit: usize,
}

fn prepare() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .try_init();
}

// TODO: console app
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    prepare();
    let args: Vec<String> = env::args().collect();
    main_inner(&args).await
}

async fn main_inner(args: &Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    info!("Scraper started with args: {:?}", args);

    let args = Args::parse_from(args);
    let dash_map = dashmap::DashMap::new();
    let walker = PageWalker::create(
        args.source_name,
        &args.etl_config_path,
        args.rule_max_depth_limit,
    )?;
    walk(&walker, &args.filter_url, args.begin_page, args.end_page, &dash_map,).await;
    info!("data {dash_map:?}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use tracing::warn;

    use super::*;

    #[test]
    fn test_arg_parser() {
        let args = Args::parse_from(["app_name_arg", "-p", "ppp", "-f", "fff", "-s", "sss"].iter());
        assert_eq!(
            args,
            Args {
                etl_config_path: "ppp".into(),
                source_name: "sss".into(),
                filter_url: "fff".into(),
                output_file_path: "./output.json".into(),
                begin_page: 1,
                end_page: 1,
                rule_max_depth_limit: 10_000,
            }
        )
    }

    #[tokio::test]
    async fn test_main() {
        prepare();
        let res = main_inner(
            &vec![
                "app_name_arg",
                "-p",
                "./etl-config.yaml",
                "-f",
                "/en/search?c=1&ob=mr&pf=0&pt=1000000",
                "-s",
                "propertyfinder",
            ]
            .into_iter()
            .map(|s| s.to_string())
            .collect(),
        )
        .await;

        assert!(res.map_err(|err| warn!("{:?}", err)).is_ok())
    }
}


#[cfg(all(test, not(feature = "ignore_dep_tests")))]
mod dep_tests {
    use super::*;

    #[tokio::test]
    async fn parse_web_page_with_scraper() -> Result<(), anyhow::Error> {
        prepare();

        let client = reqwest::Client::builder().build()?;
        let response = client
            .get("https://news.ycombinator.com/")
            .send()
            .await?
            .text()
            .await?;

        let document = scraper::Html::parse_document(&response);
        // document.root_element().select(selector)
        let title_selector = scraper::Selector::parse("span.titleline>a").unwrap();

        let titles = document.select(&title_selector).map(|x| x.inner_html());

        let titles_c = titles.collect::<Vec<String>>();
        info!("titles {titles_c:?}");

        let value = titles_c.len();
        assert_eq!(value, 30);

        Ok(())
    }

}