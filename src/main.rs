use std::io::Read;
use std::iter::Map;
use std::path::PathBuf;
use std::str::{self, FromStr};
use std::vec::Vec;
use tokio;

use clap::Parser;
use tokio::io::AsyncReadExt;

mod page_walker;
mod request_maker;
mod etl_config_parser;
mod transform_html;

/// Simple program to greet a person
#[derive(Parser, Debug, PartialEq)]
#[command(version, about, long_about = None)]
struct Args {
    /// Etl config file location
    #[arg(short='p', long, )]
    etl_config_path: PathBuf,

    /// Source name from config file
    #[arg(short, long, )]
    source_name: String,

    /// Filter url pattern
    #[arg(short, long, )]
    filter_url: String,

    /// Output file with JSON format
    #[arg(short, long, default_value="./output.json")]
    output_file_path: PathBuf,

    /// Scraping will start from begin page
    #[arg(short, long, default_value_t=1)]
    begin_page: usize,


    /// Scraping will stop on end page
    #[arg(short, long, default_value_t=1)]
    end_page: usize,


    /// Html parser max depth limit 
    #[arg(short='l', long, default_value_t=10_000)]
    html_parser_max_depth_limit: usize,

}


// TODO: console app
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    
    let args = Args::parse();
    println!("args {:?}", args);
    let mut line = "".to_string();
    let _ = tokio::io::stdin().read_to_string(&mut line);
    
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arg_parser() {
        let args = Args::parse_from(["app_name_arg", 
                "-p", "ppp",
                "-f", "fff",
                "-s", "sss",
                ].iter());
        assert_eq!(args, Args{
            etl_config_path: "ppp".into(),
            source_name: "sss".into(),
            filter_url: "fff".into(),
            output_file_path: "./output.json".into(),
            begin_page: 1,
            end_page: 1,
            html_parser_max_depth_limit: 10_000,
        })
    }

}
