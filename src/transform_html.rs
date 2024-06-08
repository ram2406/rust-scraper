

use std::fmt;
use std::collections::HashMap;
use std::{error::Error, str};
use std::vec::Vec;
use regex::Regex;
use scraper;
use serde::{Deserialize, Serialize};
use serde;
use serde_json;

use tracing::{info, debug};

#[derive(Debug, Clone)]
pub struct RecursiveError(usize);

impl fmt::Display for RecursiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "recursive limit is reached [{}]", self.0)
    }
}


impl Error for RecursiveError {}

#[derive(Debug, Clone)]
pub struct AtLeastOneTagNotFoundError(String);


impl fmt::Display for AtLeastOneTagNotFoundError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at least one tag for selector is not found [{}]", self.0)
    }
}

impl Error for AtLeastOneTagNotFoundError {}

#[derive(Debug, Default, Clone)]
pub struct ParserTransfromRule<'a> {
    selector: String,
    mapping: String,
    attribute_name: String, 
    regex_sub_value: Vec<String>,
    children: Vec<&'a ParserTransfromRule<'a>>,
    grouping: String,
    exception_on_not_found: bool,
}

impl ParserTransfromRule<'_> {
    #[inline]
    fn with_empty_selector(&self) -> Self {
        Self {
            selector: Default::default(),
            ..self.clone()
        }
    }
}

type DataMap = Box<HashMap<String, TransoftmedData>>;
type DataVec = Box<Vec<TransoftmedData>>;

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum TransoftmedData {
    Dict(DataMap),
    List(DataVec),
    Value(String),
}

const UNSUPPORTED_ENUM_TYPE: &str = "with unsupported TransformData enum type";

impl From<String> for TransoftmedData {
    fn from(value: String) -> Self {
        TransoftmedData::Value(value)
    }
}
impl From<&'_ str> for TransoftmedData {
    fn from(value: &'_ str) -> Self {
        TransoftmedData::Value(value.to_string())
    }
}
impl From<DataMap> for TransoftmedData {
    fn from(value: DataMap) -> Self {
        TransoftmedData::Dict(value)
    }
}
impl From<DataVec> for TransoftmedData {
    fn from(value: DataVec) -> Self {
        TransoftmedData::List(value)
    }
}
impl From<HashMap<String, TransoftmedData>> for TransoftmedData {
    fn from(value: HashMap<String, TransoftmedData>) -> Self {
        TransoftmedData::Dict(Box::new(value))
    }
}
impl From<Vec<TransoftmedData>> for TransoftmedData {
    fn from(value: Vec<TransoftmedData>) -> Self {
        TransoftmedData::List(Box::new(value))
    }
}
impl Into<String> for TransoftmedData {
    fn into(self) -> String {
        match self {
            TransoftmedData::Value(s) => s,
            _ => self.to_string()
        }
    }
}


impl TransoftmedData {
    pub fn create_data_map() -> DataMap { Box::new(HashMap::new()) }
    pub fn create_data_vec() -> DataVec { Box::new(Vec::new()) }
    pub fn create_dict() -> Self { TransoftmedData::Dict(TransoftmedData::create_data_map()) }
    pub fn create_list() -> Self { TransoftmedData::List(TransoftmedData::create_data_vec()) }

    pub fn prepare_dict(&mut self) -> &mut DataMap {
        let wrapper = self.as_map_wrapper();
        let dict = match wrapper {
            TransoftmedData::Dict(dict) => dict,
            _ => panic!("prepare_dict {UNSUPPORTED_ENUM_TYPE}"),
        };
        dict
    }

    pub fn is_empty(&self) -> bool {
        match self {
            TransoftmedData::Dict(dict) => dict.is_empty(),
            TransoftmedData::List(list) => list.is_empty(),
            TransoftmedData::Value(string) => string.is_empty(),
        }
    }

    pub fn as_map_wrapper(&mut self) -> &mut Self {
        match self {
            TransoftmedData::List(lst) => {
                lst.push(TransoftmedData::Dict(TransoftmedData::create_data_map()));
                let idx = lst.len() -1;
                let contained = lst.get_mut(idx);
                contained.unwrap()
            },
            TransoftmedData::Dict(dict) => self,
            _ => panic!("as_map_wrapper {UNSUPPORTED_ENUM_TYPE}"),
        }
    }

    pub fn exract_dict(&self) -> &DataMap {
        return  match self {
            TransoftmedData::Dict(dict) => dict,
            _ => panic!("extract_dict {UNSUPPORTED_ENUM_TYPE}"),
        }
    }

    pub fn exract_dict_mut(&mut self) -> &mut DataMap {
        return  match self {
            TransoftmedData::Dict(dict) => dict,
            _ => panic!("extract_dict_mut {UNSUPPORTED_ENUM_TYPE}"),
        }
    }

    #[inline]
    pub fn push_value(&mut self, key: &str, value: TransoftmedData) -> Option<&mut TransoftmedData> {
        match self {
            TransoftmedData::Dict(dict) => {
                if key.is_empty() {
                    panic!("push_value with empty key")
                }
                dict.insert(String::from(key), value);
                dict.get_mut(key)
            },
            TransoftmedData::List(lst) => {
                lst.push(value);
                let idx = lst.len() -1;
                lst.get_mut(idx)
            },
            _ => panic!("push_value {UNSUPPORTED_ENUM_TYPE}"),
        }
    }

    #[inline]
    pub fn push_value_to_list(&mut self, value: TransoftmedData) -> Option<&mut TransoftmedData> {
        self.push_value( "", value)
    }

    #[inline]
    pub fn push_value_path(&mut self, path: &str, value: TransoftmedData) -> Option<&mut TransoftmedData> {
        let path_ = path.trim_matches('.');
        if path_.is_empty() && !path.is_empty() {
            return self.push_value( path, value);    
        }
        let path = path_;
        let key_list:  Vec<&str>  = path.split('.').collect();
        if key_list.len() == 1 {
            return self.push_value(path, value);    
        }

        let mut last_data = Some(self);
        for (idx, ele) in key_list.iter().enumerate() {
            if idx == key_list.len() -1 {
                return last_data.unwrap().push_value( ele, value);
            }
            let ele_string = ele.to_string();
            let step_data = TransoftmedData::Dict(TransoftmedData::create_data_map());
                
            last_data = last_data.map(|ld| {
                let exists = !ld.is_empty() && ld.exract_dict().contains_key(&ele_string);
                if exists {
                    ld.exract_dict_mut().get_mut(&ele_string)
                } else {
                    ld.push_value( ele, step_data)
                }
            }).unwrap();
            
            
        }
        return None;
    }

    fn to_string(&self) -> String {
        serde_json::to_string(self).ok().unwrap()
    }
}

fn transform_html_single<'a, 'b>(
    transformed_data: &mut TransoftmedData,
    soup: &'b scraper::ElementRef,
    rule: &'a ParserTransfromRule<'a>,
    level: usize,
    limit: usize,
) -> Result<(), Box<dyn Error>>  {

    if level > limit {
        return Err(Box::new(RecursiveError(limit)));
    }

    let handle_regex = |rx: &Vec<String>, txt: &str| {
        let (left, right) = (&rx[0], &rx[1]);
        let regex = Regex::new(&left).unwrap();
        return regex.replace_all(txt, right,).into_owned()
    };
    let handle_attr = |selected_soup: &scraper::ElementRef, attr_name: &str| {
        let attr = selected_soup.attr(attr_name);
        return String::from(attr.unwrap_or(""));
    };
    
    let transformed_data_out: &mut TransoftmedData = if !rule.grouping.is_empty() {
        transformed_data.as_map_wrapper()
    } else {
        transformed_data
    };

    let mut selected_soup = soup;
    let mut tags: Vec<scraper::ElementRef<'_>> = Vec::new();
    
    if !rule.selector.is_empty() {
        let selector_str: &'a str = rule.selector.as_str();
        let sc_selector = scraper::Selector::parse(selector_str).unwrap();
        tags.extend(soup.select(&sc_selector));
         
        if tags.len() == 0 && rule.exception_on_not_found {
            return Err(Box::new(AtLeastOneTagNotFoundError(rule.selector.clone())));
        }
        if tags.len() == 0 {
            return Ok(());
        }
        if tags.len() > 1 {
            let nested_rule = rule.with_empty_selector();
            match transformed_data_out {
                TransoftmedData::List(lst) => {
                    for ele in tags {
                        debug!("push list one");
                        transform_html_single(transformed_data_out, &ele, &nested_rule, level+1, limit)?
                    }
                    return Ok(());
                },
                _ => {
                    let mut nested_data = TransoftmedData::List(TransoftmedData::create_data_vec());
                    for ele in tags {
                        debug!("push dict one");
                        transform_html_single(&mut nested_data, &ele, &nested_rule, level+1, limit)?
                    }
                    let key_name = if !nested_rule.grouping.is_empty() { nested_rule.grouping } 
                                           else {nested_rule.mapping};
                    
                    if !key_name.is_empty() {
                        debug!("attach list");
                        transformed_data_out.push_value_path(&key_name, nested_data);
                    }
                    return Ok(());
                },
            }
        }
        selected_soup = &tags[0];
    }

    let mappting = if !rule.mapping.is_empty() { rule.mapping.clone() }
                           else if !rule.children.is_empty() { rule.grouping.clone() }
                           else { Default::default() };
    
    if !mappting.is_empty() {
        let attr_name = if !rule.attribute_name.is_empty() { rule.attribute_name.as_str() }
                              else { "text" };
        let text = if attr_name == "text" { selected_soup.text().collect::<Vec<_>>().join(" ") } 
                           else { handle_attr(selected_soup, attr_name) };
        
        let handled_text = if !rule.regex_sub_value.is_empty() { handle_regex(&rule.regex_sub_value, &text.trim()) }
                                   else { text.trim().to_string() };
        
        transformed_data_out.push_value_path(&mappting, TransoftmedData::Value(String::from(handled_text)));
        

    }

    if !rule.children.is_empty() {
        transform_html_multi(transformed_data_out, soup, &rule.children, level, limit)?;
    }

    Ok(())
}

fn transform_html_multi<'a, 'b>(
    transoftmed_data: &mut TransoftmedData,
    soup: &'b scraper::ElementRef,
    rules: &[&'a ParserTransfromRule<'a>],
    level: usize,
    limit: usize,
) -> Result<(), Box<dyn Error>>  {
    
    for ele in rules {
        transform_html_single(transoftmed_data, soup, ele, level, limit)?
    }
    Ok(())
}

fn transform_html_inner<'a, 'b, 'c>(
    transformed_data: &'c mut TransoftmedData,
    html: &'b str,
    rules: &[&'a ParserTransfromRule<'a>],
) -> Result<(), Box<dyn Error>>   {
    let parsed = scraper::Html::parse_document(html);
    let soup = parsed.root_element();
    transform_html_multi(transformed_data, &soup, rules, 1, 100)
}

#[inline]
pub fn transform_html<'a, 'b, 'c>(
    html: &'b str,
    rules: &[&'a ParserTransfromRule<'a>],
) -> Result<DataMap, Box<dyn Error>>   {
    let mut data = TransoftmedData::Dict(TransoftmedData::create_data_map());
    transform_html_inner(&mut data, html, rules)?;
    match data {
        TransoftmedData::Dict(d) => Ok(d),
        _ => panic!("transform_html {UNSUPPORTED_ENUM_TYPE}"),
    }
}


pub fn transform_html_list<'a, 'b, 'c>(
    transformed_data: DataVec,
    html: &'b str,
    rules: &[&'a ParserTransfromRule<'a>],
) -> Result<(), Box<dyn Error>>   {
    let mut data = TransoftmedData::List(transformed_data);
    transform_html_inner(&mut data, html, rules)
}


#[cfg(test)]
mod tests {
    use tracing::Level;
    use tracing_subscriber::registry::Data;

    use super::*;

    #[test]
    fn json_test() {
        
        let mut data = TransoftmedData::create_dict();
        let lst = data.push_value_path("test.a.b", TransoftmedData::create_list()).unwrap();
        {
            lst.push_value_to_list("1".into());
            lst.push_value_to_list("2".into());
            lst.push_value_to_list("3".into());
        }
        let j = serde_json::to_string(&data).ok().unwrap();
        // std::println!("json data: {}", j);
        let json_expected = r#"{"test":{"a":{"b":["1","2","3"]}}}"#;
        assert_eq!(j, json_expected);

        let j = data.to_string();
        assert_eq!(j, json_expected);

        let j: String = data.into();
        assert_eq!(j, json_expected);
    }

    #[test]
    fn path_value_test() {
        let mut data = TransoftmedData::create_dict();
        
        data.push_value_path("another_one", "one".into());
        data.push_value_path(".", "dot".into());
        data.push_value_path("....", "dots".into());
        
        data.push_value_path("test.a.b", TransoftmedData::Value("1".to_string()));
        data.push_value_path("test.a.c", "1".into());
        data.push_value_path("test.a.d.", "2".to_string().into());
        
        std::println!("{:?}", data);
        
        let panic_caught = std::panic::catch_unwind(|| {
            let _ = TransoftmedData::create_dict().push_value_path("", "2".to_string().into());
            
        }).is_err();
        assert!(panic_caught, "expected panic after empty key");
    }



    #[test]
    fn main_test() {
        type rl<'c> = ParserTransfromRule<'c>;
        // tracing_subscriber::fmt().with_max_level(Level::DEBUG).init();
        // tracing_subscriber::fmt().with_env_filter("declarative_scraper::transform_html").init();
        


        let html = r#"
            <ul>
                <li class="test">Foo</li>
                <li>Bar</li>
                <li>Baz</li>
            </ul>
            <ul>
                <li class="test1">Foo1</li>
                <li>Bar1</li>
                <li>Baz1</li>
            </ul>
        "#;

        let data = transform_html(html, &[
            &rl{selector: String::from(".test"), mapping: String::from("place"), ..Default::default() },
            &rl{selector: String::from("ul"), mapping: String::from("ul"), ..Default::default() },
            &rl{selector: String::from("li"), mapping: String::from("lis"), ..Default::default() },

        ]).expect("Err");
        assert_eq!(data["place"], TransoftmedData::from("Foo"))
    }

    #[test]
    fn check_expand_data<'c>() {
        let consumer = |td1: &mut TransoftmedData| {
                
            // let td1_b = td1.borrow_mut();

            let  td3: &mut TransoftmedData = match td1 {
                TransoftmedData::List(lst) => {
                    lst.push(TransoftmedData::Dict(TransoftmedData::create_data_map()));
                    let idx = lst.len() -1;
                    let contained = lst.get_mut(idx);
                    contained.unwrap()
                },
                TransoftmedData::Dict(_) => td1,
                _ => td1,
            };
            
        };
        
        let mut td1 = TransoftmedData::List(TransoftmedData::create_data_vec());
        consumer(&mut td1);    
    }
}


#[cfg(test)]
mod  deps_tests {
    use std::error::Error;

    use super::*;

    // see: https://iproyal.com/blog/web-scraping-with-rust-the-ultimate-guide/
    #[tokio::test]
    async fn parse_web_page_with_scraper() -> Result<(), Box<dyn Error>> {
        
        tracing_subscriber::fmt::init();

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

    #[test]
    fn regex_test() -> Result<(), Box<dyn Error>> {
        
        tracing_subscriber::fmt::init();

        
        let re = Regex::new(r"(?m)^([^:]+):([0-9]+):(.+)$").unwrap();
        let hay = "\
path/to/foo:54:Blue Harvest
path/to/bar:90:Something, Something, Something, Dark Side
path/to/baz:3:It's a Trap!
        ";
        info!("hay [{hay}]");
        let mut results = vec![];
        for (_, [path, lineno, line]) in re.captures_iter(hay).map(|c| c.extract()) {
            results.push((path, lineno.parse::<u64>()?, line));
        }
        assert_eq!(results, vec![
            ("path/to/foo", 54, "Blue Harvest"),
            ("path/to/bar", 90, "Something, Something, Something, Dark Side"),
            ("path/to/baz", 3, "It's a Trap!"),
        ]);

        Ok(())
    }
}
