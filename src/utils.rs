use std::future::Future;
use std::time::Duration;

use bytes::Bytes;
use reqwest::Client;
use tokio::time::delay_for;

pub fn spawn_protected<F>(future: F)
where
    F: Future<Output = Result<(), Error>> + Send + 'static,
{
    tokio::spawn(async move {
        match future.await {
            Ok(()) => {}
            Err(err) => eprintln!("task failed: error: {}", err),
        }
    });
}

pub type Error = Box<dyn std::error::Error + Send + Sync>;

pub async fn fetch_url(client: &Client, url: &str) -> Result<Bytes, Error> {
    let mut attempts = 0;
    let mut last_error = None;
    loop {
        attempts += 1;
        if attempts >= 6 {
            return Err(last_error.unwrap());
        }

        let resp = match client.get(url).send().await {
            Ok(resp) => resp,
            Err(e) => {
                delay_for(Duration::from_millis(500)).await;
                last_error = Some(Error::from(e));
                continue;
            }
        };

        match resp.bytes().await {
            Ok(body) => return Ok(body),
            Err(e) => {
                delay_for(Duration::from_millis(500)).await;
                last_error = Some(Error::from(e));
                continue;
            }
        }
    }
}
