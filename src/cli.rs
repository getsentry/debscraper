use std::fs;
use std::borrow::Cow;
use std::path::PathBuf;

use chrono::Utc;
use console::style;
use structopt::StructOpt;
use tempfile::TempDir;

use crate::downloader::download_packages;
use crate::pool::ClientPool;
use crate::scrape::scrape_debian_packages;
use crate::utils::Error;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "debscraper",
    about = "Scrapes debug images from debian servers."
)]
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

    /// Bundle suffix to use
    #[structopt(long = "bundle-suffix")]
    bundle_suffix: Option<String>,

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

    fs::create_dir_all(&opt.output)?;
    let tmp_dir = TempDir::new_in(&opt.output)?;
    let tmp_dir_for_ctrlc = tmp_dir.path().to_owned();
    ctrlc::set_handler(move || {
        eprintln!("Interrupted!");
        fs::remove_dir_all(&tmp_dir_for_ctrlc).ok();
        std::process::exit(1);
    })?;

    // well known urls:
    // - http://archive.ubuntu.com/ubuntu/pool/
    // - http://ddebs.ubuntu.com/ubuntu/pool/
    println!(
        "scraping-concurrency: {}",
        style(opt.scraping_concurrency).yellow()
    );
    println!(
        "downloading-concurrency: {}",
        style(opt.downloading_concurrency).yellow()
    );
    println!("prefix: {}", style(&opt.prefix).yellow());
    println!("output: {}", style(&opt.output.display()).yellow());
    println!();

    let pool = ClientPool::new(opt.scraping_concurrency);
    let packages = scrape_debian_packages(&pool, opt.urls).await?;
    drop(pool);

    let bundle_suffix = match opt.bundle_suffix {
        Some(ref val) => Cow::Borrowed(val.as_str()),
        None => {
            let now = Utc::now();
            Cow::Owned(now.format("%Y-%m-%d").to_string())
        }
    };
    let pool = ClientPool::new(opt.downloading_concurrency);
    download_packages(&pool, packages, &opt.output, &opt.prefix, &bundle_suffix).await?;
    drop(pool);

    fs::remove_dir_all(&tmp_dir)?;

    Ok(())
}
