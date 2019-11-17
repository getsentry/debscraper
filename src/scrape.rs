use tokio::prelude::*;

use std::mem;
use std::sync::Arc;
use std::time::{Duration, Instant};

use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use lazy_static::lazy_static;
use regex::bytes::Regex;
use reqwest;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::Mutex;
use url::Url;

use crate::pool::ClientPool;
use crate::utils::{Error, spawn_protected};

lazy_static! {
    static ref LINK_RE: Regex = Regex::new(r#"(?i)\bhref="([^"]+)"#).unwrap();
}

#[derive(Debug)]
enum Link {
    Listing(String),
    Deb(String),
}

/// Given a URL returns a vector of all links that could be followed.
async fn find_links(client: &reqwest::Client, url: String) -> Result<Vec<Link>, Error> {
    let base_url = Url::parse(&url)?;
    let body = client.get(&url).send().await?.bytes().await?;
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
        if target_url.ends_with(".deb") || target_url.ends_with(".ddeb") {
            rv.push(Link::Deb(target_url));
        } else if target_url.ends_with('/') {
            rv.push(Link::Listing(target_url));
        }
    }

    Ok(rv)
}

/// Scrapes a list of URLs for all reachable debian packages.
pub async fn scrape_debian_packages(
    pool: &ClientPool,
    urls: impl IntoIterator<Item = String>,
) -> Result<Vec<String>, Error> {
    let mut sources = String::new();
    let (mut tx, mut rx) = unbounded_channel();
    for (idx, url) in urls.into_iter().enumerate() {
        use std::fmt::Write;
        if idx > 0 {
            sources.push_str(", ");
        }
        write!(&mut sources, "{}", style(&url).green())?;
        tx.try_send(url)?;
    }

    log_stage!(1, "Fetching archive indexes ({})", sources);

    let archives = Arc::new(Mutex::new(vec![]));
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner().template(" {spinner:.cyan}  {msg:.dim}\n    {prefix}"),
    );
    pb.enable_steady_tick(100);

    let started = Instant::now();
    loop {
        let new_item = rx.recv().timeout(Duration::from_millis(100)).await;
        let index_url = match new_item {
            Ok(Some(index_url)) => index_url,
            Ok(None) => break,
            Err(_) => {
                if pool.is_full() {
                    break;
                } else {
                    continue;
                }
            }
        };
        let client = pool.get_client().await;
        let archives = archives.clone();
        let pb = pb.clone();
        let mut tx = tx.clone();
        spawn_protected(async move {
            let client = client;
            let archives = archives.clone();
            let links = find_links(&client, index_url.clone()).await?;
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
            client.release().await;
            Ok(())
        });
    }

    pb.finish_and_clear();
    log_result!(
        "Found {} archives in {}s",
        style(archives.lock().await.len()).yellow(),
        started.elapsed().as_secs()
    );

    let mut archives = archives.lock().await;
    Ok(mem::replace(&mut *archives, Default::default()))
}