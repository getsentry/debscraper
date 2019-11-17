use std::future::Future;

pub fn spawn_protected<F>(future: F)
where
    F: Future<Output = Result<(), Error>> + Send + 'static,
{
    tokio::spawn(async move {
        match future.await {
            Ok(()) => {}
            Err(err) => panic!("error: {}", err),
        }
    });
}

pub type Error = Box<dyn std::error::Error + Send + Sync>;