use derive_more::{Display, Error, From};
use lazy_static::lazy_static;
use regex::Regex;
use serde;
use serde::{Deserialize, Serialize};
use serde_json;
use core::fmt;
use std::collections::HashMap;
use std::default;
use std::rc::Rc;
use std::str::{self, FromStr};
use std::vec::Vec;

#[derive(Debug)]
pub struct TransformSettings {
    pub max_depth_level: usize,
    pub default_key_name: String,
}

impl Default for TransformSettings {
    fn default() -> TransformSettings {
        TransformSettings {
            max_depth_level: 10_000,
            default_key_name: "list".into(),
        }
    }
}

/// indicates a place for separating
pub static BS_CONTAINS_MARKER: &str = "/****/";

lazy_static! {
    /// for fix regex diff from Python to Rust
    static ref RX_PAGE_NUM: Regex = Regex::new(r"\\(\d+)").expect("couldn't parse regex in [prepare_rx_sub_for_replace]");
    /// for fix shortage rust BS vestion
    static ref RX_BS_CONTAINS_PC: Regex = Regex::new(r#"(:-soup-contains\(\s?+"(.*?)"\s?+\))"#).expect("couldn't parse regex in [bs_py_adopt_contains]");
    
}

/// Convert Python replace substitution for Rust Regex
pub fn prepare_rx_sub_for_replace(origin_rx: &str) -> String {
    RX_PAGE_NUM.replace_all(origin_rx, "$$1").into_owned()
}

/// Exclude unsupport css selector part
pub fn bs_py_adopt_contains(source_selector: &str) -> String {
    RX_BS_CONTAINS_PC
        .replace_all(&source_selector, format!("{BS_CONTAINS_MARKER}/*$1*/{BS_CONTAINS_MARKER}"))
        .into_owned()
}

#[derive(Debug, Clone, Error, Display)]
#[allow(dead_code)]
pub enum TransformError {
    #[display(fmt = "recursive limit is reached [{}]", level)]
    RecursiveError { level: usize },
    #[display(fmt = "at least one tag for selector is not found [{}]", tag_name)]
    AtLeastOneTagNotFoundError { tag_name: String },
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, From, PartialEq)]
#[serde(default)]
pub struct ParserTransfromRule {
    pub selector: String,
    pub mapping: String,
    pub attribute_name: String,
    pub regex_sub_value: Vec<String>,
    pub children: Rc<Vec<Self>>,
    pub grouping: String,
    pub exception_on_not_found: bool,
}

#[allow(dead_code)]
impl ParserTransfromRule {
    #[inline]
    pub fn with_empty_selector(&self) -> Self {
        Self {
            selector: Default::default(),
            ..self.clone()
        }
    }

    #[inline]
    pub fn with_empty_grouping(&self) -> Self {
        Self {
            grouping: Default::default(),
            ..self.clone()
        }
    }

    #[inline]
    pub fn with_selector(&self, selector: &str) -> Self {
        Self {
            selector: selector.into(),
            ..self.clone()
        }
    }

    /// check is contains and return search text
    #[inline]
    pub fn is_contains_selector(&self) -> Option<(String, (String, String))> {
        let text = RX_BS_CONTAINS_PC.captures(&self.selector)
                    .map(|s| {
                        s.get(2)
                        .map(|s| s.as_str()).unwrap_or("")
                    })
                    .map(String::from);
        if text == None {
            return None;
        }
        let mut sp = RX_BS_CONTAINS_PC.splitn(&self.selector, 2)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        Some(
            ( text.unwrap(), (sp.next().unwrap(), sp.next().unwrap_or("".into())) )
        )
    }

}

#[derive(From, Debug)]
#[from(forward)]
#[allow(dead_code)]
pub struct ParserTransfromRuleError(serde_json::Error);

impl FromStr for ParserTransfromRule {
    type Err = ParserTransfromRuleError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str::<ParserTransfromRule>(s).map_err(|err| err.into())
    }
}

pub type DataMap = Box<HashMap<String, TransformedData>>;
pub type DataVec = Box<Vec<TransformedData>>;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(untagged)]
pub enum TransformedData {
    Dict(DataMap),
    List(DataVec),
    Value(String),
}

pub const UNSUPPORTED_ENUM_TYPE: &str = "with unsupported TransformData enum type";

impl From<String> for TransformedData {
    fn from(value: String) -> Self {
        TransformedData::Value(value)
    }
}
impl From<&'_ str> for TransformedData {
    fn from(value: &'_ str) -> Self {
        TransformedData::Value(value.to_string())
    }
}
impl From<DataMap> for TransformedData {
    fn from(value: DataMap) -> Self {
        TransformedData::Dict(value)
    }
}
impl From<DataVec> for TransformedData {
    fn from(value: DataVec) -> Self {
        TransformedData::List(value)
    }
}
impl From<HashMap<String, TransformedData>> for TransformedData {
    fn from(value: HashMap<String, TransformedData>) -> Self {
        TransformedData::Dict(Box::new(value))
    }
}
impl From<Vec<TransformedData>> for TransformedData {
    fn from(value: Vec<TransformedData>) -> Self {
        TransformedData::List(Box::new(value))
    }
}
impl Into<String> for TransformedData {
    fn into(self) -> String {
        match self {
            TransformedData::Value(s) => s,
            _ => self.to_json_string(),
        }
    }
}

#[allow(dead_code)]
impl TransformedData {
    pub fn create_data_map() -> DataMap {
        Box::new(HashMap::new())
    }
    pub fn create_data_vec() -> DataVec {
        Box::new(Vec::new())
    }
    pub fn create_dict() -> Self {
        TransformedData::Dict(TransformedData::create_data_map())
    }
    pub fn create_list() -> Self {
        TransformedData::List(TransformedData::create_data_vec())
    }

    pub fn prepare_dict(&mut self) -> &mut DataMap {
        let wrapper = self.as_map_wrapper();
        let dict = match wrapper {
            TransformedData::Dict(dict) => dict,
            _ => panic!("prepare_dict {UNSUPPORTED_ENUM_TYPE}"),
        };
        dict
    }

    pub fn is_empty(&self) -> bool {
        match self {
            TransformedData::Dict(dict) => dict.is_empty(),
            TransformedData::List(list) => list.is_empty(),
            TransformedData::Value(string) => string.is_empty(),
        }
    }

    pub fn as_map_wrapper(&mut self) -> &mut Self {
        match self {
            TransformedData::List(lst) => {
                lst.push(TransformedData::Dict(TransformedData::create_data_map()));
                lst.last_mut().unwrap()
            }
            TransformedData::Dict(_) => self,
            _ => panic!("as_map_wrapper {UNSUPPORTED_ENUM_TYPE}"),
        }
    }

    pub fn as_list_wrapper(&mut self, key: &str) -> &mut Self {
        match self {
            TransformedData::List(lst) => self,
            TransformedData::Dict(dict) => {
                dict.insert(key.into(), TransformedData::create_list());
                dict.get_mut(key).unwrap()
            },
            _ => panic!("as_group_list_wrapper {UNSUPPORTED_ENUM_TYPE}"),
        }
    }

    pub fn as_group_list_wrapper(&mut self, grouping_key: &str) -> &mut Self {
        if grouping_key.is_empty() {
            return self;
        }
        match self {
            TransformedData::List(lst) => {
                lst.push(TransformedData::Dict(TransformedData::create_data_map()));
                lst.last_mut().map(|dict: &mut TransformedData|{
                    dict.push_value(grouping_key, TransformedData::create_list()).unwrap()
                }).unwrap()
            },
            TransformedData::Dict(dict) => {
                dict.insert(grouping_key.into(), TransformedData::create_list());
                dict.get_mut(grouping_key).unwrap()
            },
            _ => panic!("as_group_list_wrapper {UNSUPPORTED_ENUM_TYPE}"),
        }
    }

    pub fn exract_dict(&self) -> &DataMap {
        return match self {
            TransformedData::Dict(dict) => dict,
            _ => panic!("extract_dict {UNSUPPORTED_ENUM_TYPE}"),
        };
    }

    pub fn exract_dict_mut(&mut self) -> &mut DataMap {
        return match self {
            TransformedData::Dict(dict) => dict,
            _ => panic!("extract_dict_mut {UNSUPPORTED_ENUM_TYPE}"),
        };
    }

    pub fn exract_list(&self) -> &DataVec {
        return match self {
            TransformedData::List(lst) => lst,
            _ => panic!("exract_list {UNSUPPORTED_ENUM_TYPE}"),
        };
    }

    pub fn exract_list_mut(&mut self) -> &mut DataVec {
        return match self {
            TransformedData::List(lst) => lst,
            _ => panic!("exract_list_mut {UNSUPPORTED_ENUM_TYPE}"),
        };
    }

    pub fn exract_value(&self) -> &String {
        return match self {
            TransformedData::Value(value) => value,
            _ => panic!("exract_value {UNSUPPORTED_ENUM_TYPE}"),
        };
    }

    #[inline]
    pub fn push_value(
        &mut self,
        key: &str,
        value: TransformedData,
    ) -> Option<&mut TransformedData> {
        match self {
            TransformedData::Dict(dict) => {
                if key.is_empty() {
                    panic!("push_value with empty key")
                }
                dict.insert(String::from(key), value);
                dict.get_mut(key)
            }
            TransformedData::List(lst) => {
                lst.push(value);
                lst.last_mut()
            }
            _ => panic!("push_value {UNSUPPORTED_ENUM_TYPE}"),
        }
    }

    #[inline]
    pub fn push_value_to_list(&mut self, value: TransformedData) -> Option<&mut TransformedData> {
        self.push_value("", value)
    }

    #[inline]
    pub fn push_value_path(
        &mut self,
        path: &str,
        value: TransformedData,
    ) -> Option<&mut TransformedData> {
        let path_ = path.trim_matches('.');
        if path_.is_empty() && !path.is_empty() {
            return self.push_value(path, value);
        }
        let path = path_;
        let key_list: Vec<&str> = path.split('.').collect();
        if key_list.len() == 1 {
            return self.push_value(path, value);
        }

        let mut last_data = Some(self);
        for (idx, ele) in key_list.iter().enumerate() {
            if idx == key_list.len() - 1 {
                return last_data.unwrap().push_value(ele, value);
            }
            let ele_string = ele.to_string();
            let step_data = TransformedData::Dict(TransformedData::create_data_map());

            last_data = last_data
                .map(|ld| {
                    let exists = !ld.is_empty() && ld.exract_dict().contains_key(&ele_string);
                    if exists {
                        ld.exract_dict_mut().get_mut(&ele_string)
                    } else {
                        ld.push_value(ele, step_data)
                    }
                })
                .unwrap();
        }
        return None;
    }

    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).ok().unwrap()
    }
}

impl fmt::Display for TransformedData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        
        write!(f, "{}", match self {
            TransformedData::Dict(d) => format!("Dict({})", d.len()),
            TransformedData::List(l) => format!("List({})", l.len()),
            TransformedData::Value(v) => format!("Value({})", v.len()),
        })
    }
}

#[cfg(test)]
mod tests {

    use tracing::info;

    use super::*;
    fn prepare_test_logs() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .try_init();
    }

    #[test]
    fn json_test() {
        let mut data = TransformedData::create_dict();
        let lst = data
            .push_value_path("test.a.b", TransformedData::create_list())
            .unwrap();
        {
            lst.push_value_to_list("1".into());
            lst.push_value_to_list("2".into());
            lst.push_value_to_list("3".into());
        }
        let j = serde_json::to_string(&data).ok().unwrap();
        // std::println!("json data: {}", j);
        let json_expected = r#"{"test":{"a":{"b":["1","2","3"]}}}"#;
        assert_eq!(j, json_expected);

        let j = data.to_json_string();
        assert_eq!(j, json_expected);

        let j: String = data.into();
        assert_eq!(j, json_expected);
    }

    #[test]
    fn path_value_test() {
        let mut data = TransformedData::create_dict();

        data.push_value_path("another_one", "one".into());
        data.push_value_path(".", "dot".into());
        data.push_value_path("....", "dots".into());

        data.push_value_path("test.a.b", TransformedData::Value("1".to_string()));
        data.push_value_path("test.a.c", "1".into());
        data.push_value_path("test.a.d.", "2".to_string().into());

        std::println!("{:?}", data);

        let panic_caught = std::panic::catch_unwind(|| {
            let _ = TransformedData::create_dict().push_value_path("", "2".to_string().into());
        })
        .is_err();
        assert!(panic_caught, "expected panic after empty key");
    }

    #[test]
    fn check_expand_data<'c>() {
        let consumer = |td1: &mut TransformedData| {
            // let td1_b = td1.borrow_mut();

            let _: &mut TransformedData = match td1 {
                TransformedData::List(lst) => {
                    lst.push(TransformedData::Dict(TransformedData::create_data_map()));
                    lst.last_mut().unwrap()
                }
                TransformedData::Dict(_) => td1,
                _ => td1,
            };
        };

        let mut td1 = TransformedData::List(TransformedData::create_data_vec());
        consumer(&mut td1);
    }

    #[test]
    fn contains_selector_test() {
        prepare_test_logs();
        let rule = ParserTransfromRule{ 
            selector: r#"li.property-facts__item"#.to_string(),
             ..Default::default() 
        };
        
        let _ = {
            let res = rule.is_contains_selector();
            assert_eq!(res, None);
        };
        
        let rule = ParserTransfromRule{ 
            selector: r#"li.property-facts__item:-soup-contains( "Property type:")/*("")*/ .property-facts__value"#.to_string(),
             ..Default::default() 
        };
        let Some((txt, (left, right))) = rule.is_contains_selector() else { panic!("empty result") };
        
        assert_eq!(txt, "Property type:");
        assert_eq!(left, "li.property-facts__item");
        assert_eq!(right, r#"/*("")*/ .property-facts__value"#); 
    }

    #[test]
    fn regex_replace_all_fix() -> Result<(), anyhow::Error> {
        prepare_test_logs();
        let rstr = prepare_rx_sub_for_replace(r"page-\1");
        info!("rstr = [{rstr}]");
        let rx = Regex::new(r"(\d+)")?;
        let res = rx.replace_all("123445", rstr).into_owned();
        info!("res = [{res}]");

        Ok(())
    }
}

#[cfg(test)]
#[allow(dead_code)]
mod std_tests {
    use std::error::Error;

    use derive_more::Display;

    #[test]
    fn check_traits() {
        trait MixType {
            fn test(&self);
        }

        trait MyError: Error + MixType {
            fn message(&self) -> &str {
                "MyError default msg"
            }
        }

        #[derive(Display, Debug)]
        struct MyErrorSpec {}

        impl MixType for MyErrorSpec {
            fn test(&self) {
                todo!()
            }
        }
        impl Error for MyErrorSpec {}
        impl MyError for MyErrorSpec {}

        let consumer = |me: &dyn MyError| println!("err msg: [{}]", me.message());
        let mec = MyErrorSpec {};
        consumer(&mec);
    }

    #[test]
    fn check_iter_windows<'c>() {
        let vec = vec![1, 2, 3];
        for it in vec.windows(2) {
            let [prev, cur] = it else { panic!() };
            println!("= {:?} {:?}", prev, cur);
        }
        println!("{:?}", vec);
    }
    
}
