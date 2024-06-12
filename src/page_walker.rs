use super::etl_config_parser::*;

struct PageWalker<'t> {
	source_name: String,
	etl_config_path: String,
	etl_config: &'t EtlConfig,
	source_config: &'t SourceConfig,
	// request_maker: &utils.RequestMaker,
	menu_page_url_sub: String,
}