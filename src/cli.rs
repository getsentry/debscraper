use std::path::PathBuf;

use console::style;
use structopt::StructOpt;

use crate::pool::ClientPool;
use crate::utils::Error;
use crate::scrape::scrape_debian_packages;
use crate::downloader::download_packages;

#[derive(Debug, StructOpt)]
#[structopt(name = "debscraper", about = "Scrapes debug images from debian servers.")]
struct Opt {
    /// Server URLs to pool directories.
    #[structopt(short = "u", long = "pool-url")]
    urls: Vec<String>,

    /// Number of concurrent connections for scraping.
    #[structopt(long = "scraping-concurrency", default_value = "128")]
    scraping_concurrency: usize,

    /// Number of concurrent connections for downloading.
    #[structopt(long = "downloading-concurrency", default_value = "16")]
    downloading_concurrency: usize,

    /// The prefix to use
    #[structopt(short = "p", long = "prefix")]
    prefix: String,

    /// Where to write the output files
    #[structopt(short = "o", long = "output", default_value = "./output")]
    output: PathBuf,
}

pub async fn main() -> Result<(), Error> {
    let opt = Opt::from_args();

    if opt.urls.is_empty() {
        println!("Done: no urls given.");
        return Ok(());
    }

    // well known urls:
    // - http://archive.ubuntu.com/ubuntu/pool/
    // - http://ddebs.ubuntu.com/ubuntu/pool/
    println!("scraping-concurrency: {}", style(opt.scraping_concurrency).yellow());
    println!("downloading-concurrency: {}", style(opt.downloading_concurrency).yellow());
    println!("prefix: {}", style(&opt.prefix).yellow());
    println!("output: {}", style(&opt.output.display()).yellow());
    println!();

    let pool = ClientPool::new(opt.scraping_concurrency);
    let packages = scrape_debian_packages(&pool, opt.urls).await?;
    drop(pool);

    let pool = ClientPool::new(opt.downloading_concurrency);
    download_packages(&pool, packages, &opt.output, &opt.prefix).await?;
    drop(pool);
    
    Ok(())
}
