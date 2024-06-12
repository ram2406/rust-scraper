use std::collections::HashMap;
use transform_html_lib::transform_html::defs::ParserTransfromRule;


pub struct SourceConfig {
	name:       String,
	root_url:   String,
	
    menu:       MenuRules,
    card:       CardRules,
}

pub struct MenuRules {
    page_limit:     i32,
    cards_per_page: i32,
    default_url:    String,
    page_url_sub:   String,
    first_page_url: String,

    rules:          Vec<ParserTransfromRule>,
}

pub struct CardRules {
    rules:  Vec<ParserTransfromRule>,
}

pub struct HttpConfigRetries {
	max_retries:        usize,
	backoff_factor:     usize,
	timeout:            usize,
	status_forcelist:   Vec<usize>,
}

pub struct HttpConfig {
	retries: HttpConfigRetries,
	headers: HashMap<String, String>,	
}

pub struct EtlConfig {
	http:       HttpConfig,
	sources:    Vec<SourceConfig>,
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        
    }

}