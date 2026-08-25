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
    execute_docker_with_output(executable, &["commit", container_id, &image_tag]).await?;

    let image_file = format!("{service_name}.tar");
    save_image(executable, &image_tag, &images_dir.join(&image_file)).await?;

    Ok(ServiceSnapshot {
        name: service_name.to_string(),
        image_digest: image_digest(executable, &image_tag).await?,
        image_tag,
        image_file,
    })
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
