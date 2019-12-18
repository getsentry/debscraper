use std::collections::HashMap;
use std::mem;
use std::sync::Arc;
use std::time::{Duration, Instant};

use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use lazy_static::lazy_static;
use regex::bytes::Regex;
use reqwest;
use tokio::time::timeout;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::Mutex;
use url::Url;

use crate::pool::ClientPool;
use crate::utils::{fetch_url, spawn_protected, Error};

lazy_static! {
    static ref LINK_RE: Regex = Regex::new(r#"(?i)\bhref="([^"]+)"#).unwrap();
}

#[derive(Debug)]
enum Link {
    Listing {
        url: String,
    },
    Deb {
        package: String,
        download_url: String,
    },
}

/// Given a URL returns a vector of all links that could be followed.
async fn find_links(client: &reqwest::Client, url: String) -> Result<Vec<Link>, Error> {
    let base_url = Url::parse(&url)?;
    let body = fetch_url(client, &url).await?;

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

        let target_url_s = target_url.to_string();
        if target_url_s.ends_with(".deb") || target_url_s.ends_with(".ddeb") {
            let segments = target_url.path_segments().map_or(vec![], |x| x.collect());
            rv.push(Link::Deb {
                package: segments[segments.len() - 2].to_string(),
                download_url: target_url_s,
            });
        } else if target_url_s.ends_with('/') {
            rv.push(Link::Listing { url: target_url_s });
        }
    }

    Ok(rv)
}

/// Scrapes a list of URLs for all reachable debian packages.
pub async fn scrape_debian_packages(
    pool: &ClientPool,
    urls: impl IntoIterator<Item = String>,
) -> Result<HashMap<String, Vec<String>>, Error> {
    let mut sources = String::new();
    let (tx, mut rx) = unbounded_channel();
    for (idx, url) in urls.into_iter().enumerate() {
        use std::fmt::Write;
        if idx > 0 {
            sources.push_str(", ");
        }
        write!(&mut sources, "{}", style(&url).green())?;
        tx.send(url)?;
    }

    log_stage!(1, "Fetching archive indexes ({})", sources);

    let packages = Arc::new(Mutex::new(HashMap::<String, Vec<String>>::new()));
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner().template(" {spinner:.cyan}  {msg:.dim}\n    {prefix}"),
    );
    pb.enable_steady_tick(100);

    let started = Instant::now();
    loop {
        let new_item = timeout(Duration::from_millis(100), rx.recv()).await;
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
        let packages = packages.clone();
        let pb = pb.clone();
        let tx = tx.clone();
        spawn_protected(async move {
            let client = client;
            let links = find_links(&client, index_url.clone()).await?;
            let mut new_archives = vec![];
            for link in links {
                match link {
                    Link::Deb {
                        package,
                        download_url,
                    } => {
                        new_archives.push((package, download_url));
                    }
                    Link::Listing { url } => {
                        tx.send(url).ok();
                    }
                }
            }
            pb.set_message(&index_url);
            let mut packages = packages.lock().await;
            pb.set_prefix(&format!(
                "{} packages found",
                style(packages.len()).yellow(),
            ));
            for (package, download_url) in new_archives {
                if let Some(a) = packages.get_mut(&package) {
                    a.push(download_url);
                } else {
                    packages.insert(package, vec![download_url]);
                }
            }
            drop(client);
            Ok(())
        });
    }

    pb.finish_and_clear();
    log_result!(
        "Found {} packages in {}s",
        style(packages.lock().await.len()).yellow(),
        started.elapsed().as_secs()
    );

    let mut packages = packages.lock().await;
    Ok(mem::replace(&mut *packages, Default::default()))
}
