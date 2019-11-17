#[macro_use]
mod macros;

mod scrape;
mod pool;
mod cli;
mod utils;
mod downloader;

#[tokio::main]
async fn main() -> Result<(), utils::Error> {
    cli::main().await?;
    Ok(())
}
