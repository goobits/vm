#[cfg(feature = "integration")]
use tokio::process::Command;

#[cfg(feature = "integration")]
async fn is_docker_available() -> bool {
    Command::new("docker")
        .arg("version")
        .output()
        .await
        .is_ok_and(|output| output.status.success())
}

#[cfg(feature = "integration")]
async fn run_docker_command(args: &[&str]) -> anyhow::Result<()> {
    let output = Command::new("docker").args(args).output().await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Docker command failed: {}", stderr);
    }
    Ok(())
}

#[tokio::test]
#[cfg(feature = "integration")]
async fn test_garbage_collection_frees_space() {
    if !is_docker_available().await {
        println!("Skipping test: Docker not available.");
        return;
    }

    // 1. Ensure registry is running
    server::start_registry()
        .await
        .expect("Failed to start registry");

    // 2. Pull a small image, tag it, and push it to the local registry
    let image = "alpine:latest";
    let local_image = "localhost:6000/test-gc-image:latest";

    run_docker_command(&["pull", image]).await.unwrap();
    run_docker_command(&["tag", image, local_image])
        .await
        .unwrap();
    run_docker_command(&["push", local_image]).await.unwrap();

    // 3. Delete the image from the registry via API (requires getting the digest)
    let client = reqwest::Client::new();
    let manifest_url = "http://localhost:6000/v2/test-gc-image/manifests/latest";

    let response = client
        .head(manifest_url)
        .header(
            "Accept",
            "application/vnd.docker.distribution.manifest.v2+json",
        )
        .send()
        .await
        .expect("Failed to get manifest digest");

    let digest = response
        .headers()
        .get("Docker-Content-Digest")
        .expect("No digest found")
        .to_str()
        .unwrap();

    let delete_url = format!(
        "http://localhost:6000/v2/test-gc-image/manifests/{}",
        digest
    );
    client
        .delete(&delete_url)
        .send()
        .await
        .expect("Failed to delete manifest");

    // 4. Run garbage collection
    let result = server::garbage_collect(true).await;
    assert!(
        result.is_ok(),
        "Garbage collection should run without errors"
    );

    let gc_result = result.unwrap();
    assert!(
        gc_result.bytes_freed > 0,
        "Garbage collection should free some space"
    );
    assert_eq!(
        gc_result.errors.len(),
        0,
        "There should be no errors during garbage collection"
    );

    // 5. Clean up
    server::stop_registry()
        .await
        .expect("Failed to stop registry");
    let _ = run_docker_command(&["image", "rm", image]).await;
    let _ = run_docker_command(&["image", "rm", local_image]).await;
}
