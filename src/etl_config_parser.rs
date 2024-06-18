use std::collections::HashMap;
use serde::{Deserialize, Serialize};

use crate::transform_html::defs::ParserTransfromRule;



#[derive(Serialize, Deserialize, Debug)]
pub struct SourceConfig {
	pub name:       String,
	pub root_url:   String,
	
    pub menu:       MenuRules,
    pub card:       CardRules,
}


#[derive(Serialize, Deserialize, Debug)]
pub struct MenuRules {
    pub page_limit:     i32,
    #[deprecated(note="unused in this implementation, amount of pages comes from cli")]
    #[serde(default)]
    pub cards_per_page: i32,
    pub default_url:    String,
    pub page_url_sub:   String,
    pub first_page_url: String,

    pub rules:          Vec<ParserTransfromRule>,
}


#[derive(Serialize, Deserialize, Debug)]
pub struct CardRules {
    pub rules:  Vec<ParserTransfromRule>,
}


#[derive(Serialize, Deserialize, Debug)]
pub struct HttpConfigRetries {
	pub max_retries:        u32,
	pub backoff_factor:     u32,
	pub timeout:            u32,
	pub status_forcelist:   Vec<u16>,
}


#[derive(Serialize, Deserialize, Debug)]
pub struct HttpConfig {
	pub retries: HttpConfigRetries,
	pub headers: HashMap<String, String>,	
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EtlConfig {
	pub http:       HttpConfig,
	pub sources:    Vec<SourceConfig>,
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        
    }

}