use derive_more::{Display, Error, From};
use serde;
use serde::{Deserialize, Serialize};
use serde_json;
use tracing_subscriber::field::display;
use std::collections::HashMap;
use std::error::Error;
use std::vec::Vec;
use std::str::{self, FromStr};

#[derive(Debug)]
pub struct TransformSettings {
    pub max_depth_level: usize,
}

impl Default for TransformSettings {
    fn default() -> TransformSettings {
        TransformSettings {
            max_depth_level: 10000,
        }
    }
}


#[derive(Debug, Clone, Error, Display)]
pub enum TransformError {
    #[display(fmt="recursive limit is reached [{}]", level)]
    RecursiveError {
        level: usize,
    },
    #[display(fmt = "at least one tag for selector is not found [{}]", tag_name)]
    AtLeastOneTagNotFoundError {
        tag_name: String,
    },
}


#[derive(Debug, Clone, Error, Display)]
#[display(fmt = "recursive limit is reached [{}]", level)]
pub struct RecursiveError {
    pub level: usize,
}

#[derive(Debug, Clone, Error, Display)]
#[display(fmt = "at least one tag for selector is not found [{}]", tag_name)]
pub struct AtLeastOneTagNotFoundError {
    pub tag_name: String,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, From,)]
#[serde(default)]
pub struct ParserTransfromRule {
    pub selector: String,
    pub mapping: String,
    pub attribute_name: String,
    pub regex_sub_value: Vec<String>,
    pub children: Vec<ParserTransfromRule>,
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
}

#[derive(From, Debug)]
#[from(forward)]
pub struct ParserTransfromRuleError(serde_json::Error);

impl FromStr for ParserTransfromRule {
    type Err = ParserTransfromRuleError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str::<ParserTransfromRule>(s).map_err(|err| err.into())
    }
}

pub type DataMap = Box<HashMap<String, TransformedData>>;
pub type DataVec = Box<Vec<TransformedData>>;

#[derive(Debug, Deserialize, Serialize, PartialEq)]
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
            _ => self.to_string(),
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

    pub fn to_string(&self) -> String {
        serde_json::to_string(self).ok().unwrap()
    }
}

#[cfg(test)]
mod tests {

    use super::*;

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

        let j = data.to_string();
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

    

    
}

#[cfg(test)]
mod std_tests {
    use super::*;

    #[test]
    fn check_traits() {
        trait MixType { fn test(&self); }
        
        trait MyError : Error + MixType {
            fn message(&self) -> &str { "MyError default msg"} 
        }
        
        #[derive(Display, Debug)]
        struct MyErrorSpec{}
        
        impl MixType    for MyErrorSpec {  fn test(&self) { todo!() }  }
        impl Error      for MyErrorSpec {    }
        impl MyError    for MyErrorSpec {    }

        let consumer = |me: &dyn MyError| {
            println!("err msg: [{}]", me.message())
        };
        let mec = MyErrorSpec{};
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