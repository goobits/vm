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
        "vm-snapshot/{}/{}:{}-{}",
        project_name,
        service_name,
        snapshot_name,
        uuid::Uuid::new_v4().simple()
    );
    let mut committed = false;
    let capture: Result<ServiceSnapshot> = async {
        let commit_output =
            execute_docker_with_output(executable, &["commit", container_id, &image_tag]).await?;
        committed = true;

        let image_file = format!("{service_name}.tar");
        save_image(executable, &image_tag, &images_dir.join(&image_file)).await?;
        let image_digest = match commit_image_digest(&commit_output) {
            Some(digest) => Some(digest),
            None => image_digest(executable, &image_tag).await?,
        };

        Ok(ServiceSnapshot {
            name: service_name.to_string(),
            image_digest,
            image_tag: image_tag.clone(),
            image_file,
        })
    }
    .await;

    // The archive owns the saved image; its temporary runtime tag is never retained.
    // Even a failed commit may have created its tag before reporting an error.
    if let Err(cleanup) = remove_capture_image(executable, &image_tag, committed).await {
        let message = match capture {
            Ok(_) => {
                format!("Snapshot image was saved, but temporary image cleanup failed: {cleanup}")
            }
            Err(error) => {
                format!("{error}; temporary snapshot image cleanup also failed: {cleanup}")
            }
        };
        let quote = |value: &str| format!("'{}'", value.replace('\'', "'\"'\"'"));
        return Err(VmError::validation(
            message,
            Some(format!(
                "Remove the temporary snapshot image with: {} image rm {}",
                quote(executable),
                quote(&image_tag)
            )),
        ));
    }

    capture
}

async fn remove_capture_image(executable: &str, image_tag: &str, committed: bool) -> Result<()> {
    if !committed {
        let reference = format!("reference={image_tag}");
        let images = execute_docker_with_output(
            executable,
            &["image", "ls", "--quiet", "--filter", &reference],
        )
        .await?;
        if images.is_empty() {
            return Ok(());
        }
    }
    execute_docker_with_output(executable, &["image", "rm", image_tag])
        .await
        .map(|_| ())
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
            execute_docker_streaming(executable, &["load", "-i", image_path]).await?;
            let actual = image_digest(executable, &service.image_tag).await?;
            if actual.is_none() || actual != service.image_digest {
                return Err(VmError::validation(
                    format!(
                        "Snapshot image identity does not match service '{}'",
                        service.name
                    ),
                    None::<String>,
                ));
            }
            Ok(())
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
    use crate::metadata::ServiceSnapshot;
    use std::fs;
    use std::os::unix::fs::symlink;
    use vm_core::error::Result;

    struct Engine {
        directory: tempfile::TempDir,
    }

    impl Engine {
        fn new(output: &str, failure: &str, cleanup_failure: bool, partial_commit: bool) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let executable = directory.path().join("runtime");
            // Execute an existing, immutable script. Writing an executable while
            // other tests fork can briefly leave an inherited writable descriptor
            // in a child and make Linux exec fail with ETXTBSY.
            symlink(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/snapshot-engine.sh"),
                &executable,
            )
            .unwrap();
            for (name, value) in [
                ("output", output.to_owned()),
                ("failure", failure.to_owned()),
                ("cleanup-failure", cleanup_failure.to_string()),
                ("partial-commit", partial_commit.to_string()),
            ] {
                fs::write(directory.path().join(name), value).unwrap();
            }
            Self { directory }
        }

        async fn snapshot(&self) -> Result<ServiceSnapshot> {
            snapshot_container(
                self.directory.path().join("runtime").to_str().unwrap(),
                "demo",
                "stable",
                "app",
                "container-id",
                self.directory.path(),
            )
            .await
        }

        fn commands(&self) -> Vec<String> {
            fs::read_to_string(self.directory.path().join("commands.log"))
                .unwrap()
                .lines()
                .map(str::to_owned)
                .collect()
        }

        fn assert_exact_cleanup(&self) -> String {
            let commands = self.commands();
            let tag = commands[0].strip_prefix("commit container-id ").unwrap();
            assert_eq!(commands.last().unwrap(), &format!("image rm {tag}"));
            assert_eq!(
                commands
                    .iter()
                    .filter(|line| line.starts_with("image rm "))
                    .count(),
                1
            );
            tag.to_string()
        }
    }

    #[tokio::test]
    async fn valid_commit_digest_avoids_an_image_inspect_and_removes_tag() {
        let committed = format!("sha256:{}", "a".repeat(64));
        let engine = Engine::new(&committed, "", false, false);

        let snapshot = engine.snapshot().await.unwrap();

        assert_eq!(snapshot.image_digest.as_deref(), Some(committed.as_str()));
        assert_eq!(snapshot.image_tag, engine.assert_exact_cleanup());
        assert_eq!(engine.commands().len(), 3);
        assert!(engine.commands()[1].starts_with(&format!("save {} -o ", snapshot.image_tag)));
    }

    #[tokio::test]
    async fn empty_or_malformed_commit_output_falls_back_to_inspect_then_removes_tag() {
        let fallback = format!("sha256:{}", "b".repeat(64));
        for output in ["", "sha256:short", "unexpected output"] {
            let engine = Engine::new(output, "", false, false);

            let snapshot = engine.snapshot().await.unwrap();

            assert_eq!(snapshot.image_digest.as_deref(), Some(fallback.as_str()));
            assert_eq!(engine.commands().len(), 4);
            assert_eq!(
                engine.commands()[2],
                format!("image inspect --format={{{{.Id}}}} {}", snapshot.image_tag)
            );
            engine.assert_exact_cleanup();
        }
    }

    #[tokio::test]
    async fn failed_save_or_digest_preserves_failure_and_removes_tag() {
        for failure in ["save", "inspect"] {
            let engine = Engine::new("", failure, false, false);

            let error = engine.snapshot().await.unwrap_err();

            assert!(matches!(error, vm_core::error::VmError::Command(_)));
            assert!(error
                .to_string()
                .contains(&format!("{failure} fixture failure")));
            engine.assert_exact_cleanup();
        }
    }

    #[tokio::test]
    async fn cleanup_failures_preserve_original_failure_and_exact_recovery_command() {
        for failure in ["", "save", "inspect"] {
            let engine = Engine::new("", failure, true, false);

            let error = engine.snapshot().await.unwrap_err();

            assert!(
                error.to_string().contains("cleanup fixture failure"),
                "capture failure {failure:?}: expected cleanup diagnostic, got {error:?}"
            );
            if !failure.is_empty() {
                assert!(
                    error
                        .to_string()
                        .contains(&format!("{failure} fixture failure")),
                    "capture failure {failure:?}: original diagnostic lost: {error:?}"
                );
            }
            let tag = engine.assert_exact_cleanup();
            assert_eq!(
                error.hint().unwrap(),
                format!(
                    "Remove the temporary snapshot image with: '{}' image rm '{tag}'",
                    engine.directory.path().join("runtime").display()
                )
            );
        }
    }

    #[tokio::test]
    async fn failed_commit_cleans_up_only_if_its_tag_exists() {
        for partial in [false, true] {
            let engine = Engine::new("", "commit", false, partial);

            let error = engine.snapshot().await.unwrap_err();

            assert!(matches!(error, vm_core::error::VmError::Command(_)));
            assert!(error.to_string().contains("commit fixture failure"));
            let commands = engine.commands();
            let tag = commands[0].strip_prefix("commit container-id ").unwrap();
            assert_eq!(
                commands[1],
                format!("image ls --quiet --filter reference={tag}")
            );
            assert!(!commands.iter().any(|line| line.starts_with("save ")));
            if partial {
                engine.assert_exact_cleanup();
            } else {
                assert_eq!(commands.len(), 2);
            }
        }
    }

    #[tokio::test]
    async fn repeated_captures_use_and_remove_distinct_tags() {
        let engine = Engine::new("", "", false, false);

        let first = engine.snapshot().await.unwrap();
        let second = engine.snapshot().await.unwrap();

        assert_ne!(first.image_tag, second.image_tag);
        let removed: Vec<_> = engine
            .commands()
            .into_iter()
            .filter_map(|line| line.strip_prefix("image rm ").map(str::to_owned))
            .collect();
        assert_eq!(removed, vec![first.image_tag, second.image_tag]);
    }
}
