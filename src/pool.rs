use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use futures_intrusive::sync::Semaphore;
use reqwest::Client;
use tokio::{timer, clock};
use tokio::sync::Mutex;

struct ClientPoolInner {
    semaphore: Semaphore,
    clients: Mutex<Vec<Option<Client>>>,
}

pub struct ClientPool {
    size: usize,
    inner: Arc<ClientPoolInner>,
}

pub struct ClientRef {
    inner: Arc<ClientPoolInner>,
    client: Option<Client>,
}

impl Deref for ClientRef {
    type Target = Client;

    fn deref(&self) -> &Client {
        self.client.as_ref().unwrap()
    }
}

impl ClientRef {
    pub async fn release(mut self) {
        for slot in self.inner.clients.lock().await.iter_mut() {
            if slot.is_none() {
                *slot = self.client.take();
            }
        }
        self.inner.semaphore.release(1);
    }
}

impl ClientPool {
    pub fn new(size: usize) -> ClientPool {
        ClientPool {
            size,
            inner: Arc::new(ClientPoolInner {
                semaphore: Semaphore::new(true, size),
                clients: Mutex::new(Vec::new()),
            }),
        }
    }

    pub fn is_full(&self) -> bool {
        self.inner.semaphore.permits() == self.size
    }

    pub async fn join(&self) {
        loop {
            if self.is_full() {
                break;
            }
            timer::delay(clock::now() + Duration::from_millis(100)).await;
        }
    }

    pub async fn get_client(&self) -> ClientRef {
        self.inner.semaphore.acquire(1).await.disarm();
        let mut clients = self.inner.clients.lock().await;

        // reuse an existing connection
        for slot in clients.iter_mut() {
            if slot.is_some() {
                return ClientRef {
                    inner: self.inner.clone(),
                    client: slot.take(),
                };
            }
        }

        // create a new connection.
        let client = Client::new();
        clients.push(None);
        ClientRef {
            inner: self.inner.clone(),
            client: Some(client),
        }
    }
}
