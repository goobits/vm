use std::path::Path;

use futures_util::stream::{self, StreamExt};
use vm_core::error::{Result, VmError};

use crate::docker::{execute_docker_streaming, execute_docker_with_output};
use crate::manager::snapshot_file_path;
use crate::metadata::ServiceSnapshot;
use crate::optimal_concurrency;

pub(crate) async fn snapshot_container(
    executable: &str,
    project_name: &str,
    snapshot_name: &str,
    service_name: &str,
    container_id: &str,
    images_dir: &Path,
) -> Result<ServiceSnapshot> {
    tracing::info!("  Snapshotting container: {}", service_name);

    let image_tag = format!(
        "vm-snapshot/{}/{}:{}",
        project_name, service_name, snapshot_name
    );
    let commit_output =
        execute_docker_with_output(executable, &["commit", container_id, &image_tag]).await?;

    let image_file = format!("{service_name}.tar");
    save_image(executable, &image_tag, &images_dir.join(&image_file)).await?;

    Ok(ServiceSnapshot {
        name: service_name.to_string(),
        image_digest: match commit_image_digest(&commit_output) {
            Some(digest) => Some(digest),
            None => image_digest(executable, &image_tag).await?,
        },
        image_tag,
        image_file,
    })
}

fn commit_image_digest(output: &str) -> Option<String> {
    let digest = output.trim();
    let encoded = digest.strip_prefix("sha256:")?;
    (encoded.len() == 64
        && encoded
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
    .then(|| digest.to_string())
}

pub(crate) async fn load_service_images(
    executable: &str,
    images_dir: &Path,
    services: &[ServiceSnapshot],
) -> Result<()> {
    let load_futures = services.iter().map(|service| {
        let service = service.clone();
        let images_dir = images_dir.to_path_buf();
        async move {
            tracing::info!("  Loading image: {}", service.name);
            let image_path = snapshot_file_path(&images_dir, &service.image_file, "image file")?;
            let image_path = path_argument(&image_path)?;
            execute_docker_streaming(executable, &["load", "-i", image_path]).await
        }
    });

    stream::iter(load_futures)
        .buffer_unordered(optimal_concurrency())
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    Ok(())
}

pub(crate) async fn save_image_streaming(
    executable: &str,
    image_tag: &str,
    destination: &Path,
) -> Result<()> {
    let destination = path_argument(destination)?;
    execute_docker_streaming(executable, &["save", image_tag, "-o", destination]).await
}

pub(crate) async fn image_digest(executable: &str, image_tag: &str) -> Result<Option<String>> {
    let digest = execute_docker_with_output(
        executable,
        &["image", "inspect", "--format={{.Id}}", image_tag],
    )
    .await?;
    Ok((!digest.is_empty()).then_some(digest))
}

async fn save_image(executable: &str, image_tag: &str, destination: &Path) -> Result<()> {
    let destination = path_argument(destination)?;
    execute_docker_with_output(executable, &["save", image_tag, "-o", destination])
        .await
        .map(|_| ())
}

fn path_argument(path: &Path) -> Result<&str> {
    path.to_str().ok_or_else(|| {
        VmError::general(
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid UTF-8 in path"),
            format!(
                "Snapshot path contains invalid UTF-8 characters: {}",
                path.display()
            ),
        )
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::snapshot_container;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    async fn snapshot_with_commit_output(commit_output: &str) -> (Option<String>, String) {
        let temp_dir = tempfile::tempdir().unwrap();
        let executable = temp_dir.path().join("runtime");
        let log = temp_dir.path().join("commands.log");
        let fallback = format!("sha256:{}", "b".repeat(64));
        fs::write(
            &executable,
            format!(
                r#"#!/bin/sh
echo "$@" >> '{}'
if [ "$1" = commit ]; then printf '%s' '{}'; exit 0; fi
if [ "$1" = image ]; then printf '%s' '{}'; exit 0; fi
exit 0
"#,
                log.display(),
                commit_output,
                fallback
            ),
        )
        .unwrap();
        let mut permissions = fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).unwrap();

        let snapshot = snapshot_container(
            executable.to_str().unwrap(),
            "demo",
            "stable",
            "app",
            "container-id",
            temp_dir.path(),
        )
        .await
        .unwrap();

        (snapshot.image_digest, fs::read_to_string(log).unwrap())
    }

    #[tokio::test]
    async fn valid_commit_digest_avoids_an_image_inspect() {
        let committed = format!("sha256:{}", "a".repeat(64));

        let (digest, commands) = snapshot_with_commit_output(&committed).await;

        assert_eq!(digest.as_deref(), Some(committed.as_str()));
        assert_eq!(commands.lines().count(), 2);
        assert!(!commands.lines().any(|line| line.starts_with("image ")));
    }

    #[tokio::test]
    async fn empty_or_malformed_commit_output_falls_back_to_inspect() {
        let fallback = format!("sha256:{}", "b".repeat(64));
        for output in ["", "sha256:short", "unexpected output"] {
            let (digest, commands) = snapshot_with_commit_output(output).await;

            assert_eq!(digest.as_deref(), Some(fallback.as_str()));
            assert_eq!(commands.lines().count(), 3);
            assert!(commands
                .lines()
                .any(|line| line == "image inspect --format={{.Id}} vm-snapshot/demo/app:stable"));
        }
    }
}
