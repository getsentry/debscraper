#[macro_use]
mod macros;
mod scrape;
mod pool;
mod utils;

#[tokio::main]
async fn main() -> Result<(), utils::Error> {
    let urls = vec![
        "http://archive.ubuntu.com/ubuntu/pool/".to_string(),
        "http://ddebs.ubuntu.com/ubuntu/pool/".to_string(),
    ];
    let pool = pool::ClientPool::new(256);
    let _result = scrape::scrape_debian_packages(&pool, urls).await?;
    Ok(())
}
