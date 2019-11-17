use async_std::sync::{Arc, Mutex};
use async_std::future::timeout;
use async_std::task;
use console::style;
use std::time::{Instant, Duration};
use indicatif::{ProgressBar, ProgressStyle};
use lazy_static::lazy_static;
use regex::bytes::Regex;
use std::future::Future;
use tokio::sync::mpsc::unbounded_channel;
use futures_intrusive::sync::Semaphore;
use url::Url;

lazy_static! {
    static ref LINK_RE: Regex = Regex::new(r#"(?i)\bhref="([^"]+)"#).unwrap();
}

type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug)]
enum Link {
    Listing(String),
    Deb(String),
}

fn spawn_protected<F>(future: F) -> task::JoinHandle<()>
where
    F: Future<Output = Result<(), Error>> + Send + 'static,
{
    task::spawn(async move {
        match future.await {
            Ok(()) => (),
            // XXX: log here
            Err(err) => {
                eprintln!("error: {}", err);
            }
        }
    })
}

/// Given a URL returns a vector of all links that could be followed.
async fn find_links(url: String) -> Result<Vec<Link>, Error> {
    let base_url = Url::parse(&url)?;
    let mut resp = surf::get(url).await?;
    let body = resp.body_bytes().await?;
    let mut rv = vec![];

    for m in LINK_RE.captures_iter(&body) {
        let target = match std::str::from_utf8(&m[1]) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let target_url = match base_url.join(target) {
            Ok(url) if url.query().is_none() => url,
            _ => continue,
        };
        if target_url.origin() != base_url.origin() {
            continue;
        }
        if target_url.path().matches('/').count() < base_url.path().matches('/').count() {
            continue;
        }

        let target_url = target_url.to_string();
        if target_url.ends_with(".deb") {
            rv.push(Link::Deb(target_url));
        } else if target_url.ends_with('/') {
            rv.push(Link::Listing(target_url));
        }
    }

    Ok(rv)
}

/// Scrapes a list of URLs for all reachable debian packages.
async fn scrape_debian_packages(url: String) -> Result<Vec<String>, Error> {
    println!(
        "[{}] Fetching archive index ({})",
        style("1").cyan().bold(),
        style(&url).green()
    );

    let archives = Arc::new(Mutex::new(vec![]));
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner().template(" {spinner:.cyan}  {msg:.dim}\n    {prefix}"),
    );
    pb.enable_steady_tick(100);

    let (mut tx, mut rx) = unbounded_channel();
    tx.try_send(url)?;

    let started = Instant::now();
    let concurrency = 32;
    let semaphore = Arc::new(Semaphore::new(true, concurrency));
    loop {
        let semaphore = semaphore.clone();
        let new_item = timeout(Duration::from_millis(100), rx.recv()).await;
        let index_url = match new_item {
            Ok(Some(index_url)) => index_url,
            Ok(None) => break,
            Err(_) => {
                if semaphore.permits() == concurrency {
                    break;
                } else {
                    continue;
                }
            }
        };
        semaphore.acquire(1).await.disarm();
        let archives = archives.clone();
        let pb = pb.clone();
        let mut tx = tx.clone();
        spawn_protected(async move {
            let archives = archives.clone();
            let links = find_links(index_url.clone()).await?;
            let mut new_archives = vec![];
            for link in links {
                match link {
                    Link::Deb(url) => new_archives.push(url),
                    Link::Listing(url) => {
                        tx.try_send(url).ok();
                    }
                }
            }
            pb.set_message(&index_url);
            let mut archives = archives.lock().await;
            pb.set_prefix(&format!(
                "{} archives found",
                style(archives.len()).yellow(),
            ));
            archives.extend(new_archives);
            semaphore.release(1);
            Ok(())
        });
    }

    pb.finish_with_message(&format!("finished scraping in {}s", started.elapsed().as_secs()));

    Ok(vec![])
}

fn main() {
    task::block_on(async {
        //"http://archive.ubuntu.com/ubuntu/pool/main/a/a11y-profile-manager".to_string(),
        //"http://archive.ubuntu.com/ubuntu/pool/".to_string(),
        //"http://ddebs.ubuntu.com/ubuntu/pool/".to_string(),
        let result =
            scrape_debian_packages("http://archive.ubuntu.com/ubuntu/pool/main/".to_string())
                .await
                .unwrap();
    });
}
