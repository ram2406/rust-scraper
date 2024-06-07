use tokio;
use std::str;
use std::iter::Map;
use std::vec::Vec;
mod transform_html;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

// code goes here

    Ok(())
}
