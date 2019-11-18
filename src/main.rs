#[macro_use]
mod macros;

mod cli;
mod downloader;
mod pool;
mod scrape;
mod utils;

#[tokio::main]
async fn main() -> Result<(), utils::Error> {
    cli::main().await?;
    Ok(())
}
