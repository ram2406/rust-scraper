use tokio;
use std::str;
use std::iter::Map;
use std::vec::Vec;
mod transform_html;

// TODO: console app
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    tracing_subscriber::fmt::init();
    Ok(())
}
