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

    /// Number of concurrent connections.
    #[structopt(short = "c", long = "concurrency", default_value = "128")]
    concurrency: usize,

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
    println!("concurrency: {}", style(opt.concurrency).yellow());
    println!("prefix: {}", style(&opt.prefix).yellow());
    println!("output: {}", style(&opt.output.display()).yellow());
    println!();

    let pool = ClientPool::new(opt.concurrency);
    let packages = scrape_debian_packages(&pool, opt.urls).await?;
    download_packages(&pool, packages, &opt.output, &opt.prefix).await?;
    
    Ok(())
}
