use std::iter::Map;
use std::str;
use std::vec::Vec;
use tokio;
mod transform_html;

// TODO: console app
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    Ok(())
}
