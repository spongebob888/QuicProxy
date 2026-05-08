use std::time::Duration;
use tokio::net::TcpListener;

#[tokio::test]
async fn test_bind() {
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let done_clone = done.clone();

    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(5));
        if !done_clone.load(std::sync::atomic::Ordering::SeqCst) {
            eprintln!("WATCHDOG: Test 'test_bind' timed out after 5s");
            std::process::abort();
        }
    });

    let test_fut = async {
        let _listener1 = TcpListener::bind("127.0.0.1:50001").await.unwrap();
        println!("Listener 1 bound");

        match TcpListener::bind("127.0.0.1:50001").await {
            Ok(_) => println!("Listener 2 bound (unexpected!)"),
            Err(e) => println!("Listener 2 failed as expected: {}", e),
        }
    };

    tokio::time::timeout(Duration::from_secs(5), test_fut)
        .await
        .expect("test_bind timed out");

    done.store(true, std::sync::atomic::Ordering::SeqCst);
}
