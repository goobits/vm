use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use vm_package_server::run_server_with_shutdown;
use reqwest::Client;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore]
async fn test_blocking_io_performance() {
    // 1. Setup
    let temp_dir = tempfile::tempdir().unwrap();
    let data_dir = temp_dir.path().to_path_buf();

    // Create MANY files to make directory walking slow
    // 50,000 files.
    let cargo_index = data_dir.join("cargo/index");
    std::fs::create_dir_all(&cargo_index).unwrap();
    println!("Generating 50,000 test files...");

    // Create directories first
    for i in 0..100 {
        std::fs::create_dir_all(cargo_index.join(format!("{}", i))).unwrap();
    }

    for i in 0..50000 {
        let dir = cargo_index.join(format!("{}", i % 100));
        std::fs::write(dir.join(format!("pkg{}", i)), "{}").unwrap();
    }
    println!("Files generated.");

    // Find free port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener); // release port

    // Start server
    let (tx, rx) = tokio::sync::oneshot::channel();
    let data_dir_clone = data_dir.clone();

    println!("Starting server on port {}...", port);
    let server_handle = tokio::spawn(async move {
        run_server_with_shutdown("127.0.0.1".to_string(), port, data_dir_clone, Some(rx)).await
    });

    // Wait for server to start
    let client = Client::new();
    let base_url = format!("http://127.0.0.1:{}", port);

    let mut retries = 0;
    loop {
        if client.get(format!("{}/status", base_url)).send().await.is_ok() {
            break;
        }
        if retries > 50 {
            panic!("Server failed to start");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        retries += 1;
    }
    println!("Server started.");

    // 2. Trigger blocking operation
    let url_packages = format!("{}/api/packages", base_url);

    let client_slow = client.clone();
    let task_slow = tokio::spawn(async move {
        let start = Instant::now();
        let _ = client_slow.get(&url_packages).send().await;
        start.elapsed()
    });

    // 3. Measure fast operation from a separate thread
    // We use spawn_blocking to simulate an external client or a client on a different thread
    // that is NOT blocked by the single-threaded runtime.
    println!("Sending health check from blocking thread...");
    let health_url = format!("{}/health", base_url);

    let handle = tokio::task::spawn_blocking(move || {
        // Sleep to ensure slow request started and blocked the runtime
        std::thread::sleep(Duration::from_millis(100));

        let client = reqwest::blocking::Client::new();
        let start = Instant::now();
        // This request will hang if the server thread is blocked
        let resp = client.get(health_url).send().expect("Failed to send health check");
        let duration = start.elapsed();
        (resp.status(), duration)
    });

    let (status, duration) = handle.await.unwrap();

    assert!(status.is_success());

    // 4. Cleanup
    tx.send(()).unwrap();

    // 5. Assertions
    println!("Health check took: {:?}", duration);
    let slow_duration = task_slow.await.unwrap();
    println!("List packages took: {:?}", slow_duration);

    if slow_duration < Duration::from_millis(100) {
        println!("Warning: Listing packages was too fast ({:?}) to measure blocking effectively.", slow_duration);
    }

    assert!(duration < Duration::from_millis(500),
            "Health check took too long: {:?}. Blocked by list_packages ({:?}).",
            duration, slow_duration);
}
