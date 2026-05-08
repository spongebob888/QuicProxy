use std::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::debug;

static HANDLES: Mutex<Vec<JoinHandle<()>>> = Mutex::new(Vec::new());

pub fn spawn(future: impl std::future::Future<Output = ()> + Send + 'static) {
    let handle = tokio::spawn(future);
    if let Ok(mut handles) = HANDLES.lock() {
        handles.push(handle);
    }
}

pub async fn abort_all_and_wait() {
    let handles: Vec<JoinHandle<()>> = {
        let mut lock = HANDLES.lock().unwrap_or_else(|e| {
            tracing::error!("HANDLES lock poisoned, recovering: {:?}", e);
            e.into_inner()
        });
        std::mem::take(&mut *lock)
    };

    for handle in &handles {
        handle.abort();
    }

    debug!("signal {} handles to close", handles.len());

    for handle in handles {
        let _ = handle.await;
    }
}
