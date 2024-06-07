

use std::fmt;
use std::collections::HashMap;
use std::{error::Error, str};
use std::vec::Vec;
use regex::Regex;
use scraper;

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

#[derive(Debug,)]
pub enum TransoftmedData {
    Dict(DataMap),
    List(DataVec),
    Value(String),
}

impl TransoftmedData {
    pub fn create_data_map() -> DataMap { Box::new(HashMap::new()) }
    pub fn create_data_vec() -> DataVec { Box::new(Vec::new()) }


    pub fn prepare_dict(&mut self) -> &mut DataMap {
        let wrapper = self.as_map_wrapper();
        let dict = match wrapper {
            TransoftmedData::Dict(dict) => dict,
            _ => panic!(),
        };
        dict
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
            _ => panic!(""),
        }
    }

    pub fn exract_dict(&mut self) -> &mut HashMap<String, TransoftmedData> {
        return  match self {
            TransoftmedData::Dict(dict) => dict,
            _ => panic!(""),
        }
    }

    #[inline]
    pub fn push_value(&mut self, value: TransoftmedData, key: &str) -> Option<&mut TransoftmedData> {
        match self {
            TransoftmedData::Dict(dict) => {
                if key.is_empty() {
                    panic!()
                }
                dict.insert(String::from(key), value);
                dict.get_mut(key)
            },
            TransoftmedData::List(lst) => {
                lst.push(value);
                let idx = lst.len() -1;
                lst.get_mut(idx)
            },
            _ => panic!(),
        }
    }

    #[inline]
    pub fn push_value_to_list(&mut self, value: TransoftmedData) -> Option<&mut TransoftmedData> {
        self.push_value(value, "")
    }

    #[inline]
    pub fn push_value_path(&mut self, path: &str, value: TransoftmedData) {
        let key_list:  Vec<&str>  = path.split('.').collect();
        if key_list.len() == 1 {
            self.push_value(value, path);    
            return
        }

        let mut last_data = Some(self);
        for (idx, ele) in key_list.iter().enumerate() {
            if idx == key_list.len() -1 {
                last_data.unwrap().push_value(value, ele);
                break;
            }
            let step_data = TransoftmedData::Dict(TransoftmedData::create_data_map());
            last_data = last_data.unwrap().push_value(step_data, ele);
            
        }
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
        
        let handled_text = handle_regex(&rule.regex_sub_value, &text.trim());
        
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
        _ => panic!(),
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

    use super::*;

    #[test]
    fn test() {
        type rl<'c> = ParserTransfromRule<'c>;
        tracing_subscriber::fmt().with_max_level(Level::DEBUG).init();
        

        let html = r#"
            <ul>
                <li class="test">Foo</li>
                <li>Bar</li>
                <li>Baz</li>
            </ul>
        "#;

        let data = transform_html(html, &[
            &rl{selector: String::from(".test"), mapping: String::from("place"), ..Default::default() },
            &rl{selector: String::from("ul"), mapping: String::from("ul"), ..Default::default() },
            &rl{selector: String::from("li"), mapping: String::from("lis"), ..Default::default() },

        ]).expect("Err");
        println!(" > > > {:?}", data);
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

// Troubles with collection & refs without Boxing or Arc
// see: https://users.rust-lang.org/t/how-to-deal-with-cow-str-in-a-hashmap-vector/87223/7
#[cfg(test)]
mod example {

    use std::borrow::Cow;
    use std::collections::{HashMap, hash_map::Entry};
    use std::mem::take;
    use urlencoding::decode as url_decode;

    #[derive(Debug)]
    pub struct QueryString<'buf> {
        data: HashMap<Cow<'buf, str>, Value<'buf>>,
    }

    #[derive(Debug)]
    pub enum Value<'buf> {
        Single(Cow<'buf, str>),
        Multiple(Vec<Cow<'buf, str>>),
    }

    impl<'buf> QueryString<'buf> {
        pub fn get(&self, key: &str) -> Option<&Value> {
            self.data.get(key)
        }
    }

    impl<'buf> From<&'buf str> for QueryString<'buf> {
        fn from(s: &'buf str) -> Self {
            let mut data = HashMap::new();

            for sub_str in s.split('&') {
                let mut parts = sub_str.splitn(2, '=');
                let key = decode(parts.next().unwrap_or_default());
                let val = decode(parts.next().unwrap_or_default());

                match data.entry(key) {
                    Entry::Occupied(mut entry) => {
                        let existing = entry.get_mut();
                        match existing {
                            Value::Single(prev_val) => {
                                *existing = Value::Multiple(vec![take(prev_val), val]);
                            }
                            Value::Multiple(vec) => vec.push(val),
                        };
                    },
                    Entry::Vacant(entry) => {
                        entry.insert(Value::Single(val));
                    }
                };
            }

            QueryString { data }
        }
    }

    fn decode<'a>(str: &'a str) -> Cow<'a, str> {
        match url_decode(str) {
            Ok(decoded) => decoded,
            Err(_) => str.into()
        }
    }

    use std::println;

    use super::TransoftmedData;

    #[test]
    fn example_test() {
        let q = QueryString::from("https://www.google.com/xs/uu/search?q=rust+errors+crate&q=rust+errors+crate&q=rust+errors+crate");
        println!("{:?}", q)

    }

    

}