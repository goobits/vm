use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use futures_util::StreamExt;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::io::ReaderStream;
use vm_packages::{
    validate_sha256, validate_tool_name, validate_tool_target, validate_tool_version,
};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

const MAX_TOOL_ARTIFACT_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct ArtifactPath {
    tool: String,
    version: String,
    target: String,
    archive: String,
}

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new().route(
        "/tools/artifacts/{tool}/{version}/{target}/{archive}",
        get(download_artifact).put(upload_artifact),
    )
}

async fn upload_artifact(
    State(state): State<Arc<AppState>>,
    Path(path): Path<ArtifactPath>,
    headers: HeaderMap,
    body: Body,
) -> AppResult<impl IntoResponse> {
    crate::auth::validate_publish_headers(&state.config, &headers)?;
    let digest = validate_artifact_path(&path)?.to_string();
    if content_length(&headers).is_some_and(|size| size > MAX_TOOL_ARTIFACT_BYTES) {
        return Err(AppError::UploadError(format!(
            "tool artifact exceeds the {} byte limit",
            MAX_TOOL_ARTIFACT_BYTES
        )));
    }

    let destination = artifact_file(&state.data_dir, &path, &digest);
    let created = save_stream_immutable(destination, body, &digest).await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, [("x-checksum-sha256", digest)]))
}

async fn download_artifact(
    State(state): State<Arc<AppState>>,
    Path(path): Path<ArtifactPath>,
    headers: HeaderMap,
) -> AppResult<Response> {
    crate::auth::validate_read_headers(&state.config, &headers)?;
    let digest = validate_artifact_path(&path)?;
    let file_path = artifact_file(&state.data_dir, &path, digest);
    let file = tokio::fs::File::open(&file_path).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            AppError::NotFound(format!(
                "tool artifact {}@{} for {}",
                path.tool, path.version, path.target
            ))
        } else {
            error.into()
        }
    })?;
    let size = file.metadata().await?.len();
    let stream = ReaderStream::new(file);
    let mut response = Body::from_stream(stream).into_response();
    let response_headers = response.headers_mut();
    response_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/gzip"),
    );
    response_headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&size.to_string())
            .map_err(|error| AppError::InternalError(error.to_string()))?,
    );
    response_headers.insert(
        header::ETAG,
        HeaderValue::from_str(&format!("\"{digest}\""))
            .map_err(|error| AppError::InternalError(error.to_string()))?,
    );
    response_headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=31536000, immutable"),
    );
    response_headers.insert(
        "x-checksum-sha256",
        HeaderValue::from_str(digest)
            .map_err(|error| AppError::InternalError(error.to_string()))?,
    );
    Ok(response)
}

fn validate_artifact_path(path: &ArtifactPath) -> AppResult<&str> {
    validate_tool_name(&path.tool).map_err(invalid)?;
    validate_tool_version(&path.version).map_err(invalid)?;
    validate_tool_target(&path.target).map_err(invalid)?;
    let digest = path
        .archive
        .strip_suffix(".tar.gz")
        .ok_or_else(|| AppError::BadRequest("tool archive must end in .tar.gz".into()))?;
    validate_sha256(digest).map_err(invalid)?;
    Ok(digest)
}

fn invalid(error: vm_packages::PackageValidationError) -> AppError {
    AppError::BadRequest(error.to_string())
}

fn artifact_file(root: &FsPath, path: &ArtifactPath, digest: &str) -> PathBuf {
    root.join("tools/artifacts")
        .join(&path.tool)
        .join(&path.version)
        .join(&path.target)
        .join(format!("{digest}.tar.gz"))
}

fn content_length(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
}

async fn save_stream_immutable(
    destination: PathBuf,
    body: Body,
    expected_digest: &str,
) -> AppResult<bool> {
    let parent = destination
        .parent()
        .ok_or_else(|| AppError::InternalError("tool artifact has no parent directory".into()))?;
    tokio::fs::create_dir_all(parent).await?;
    let temporary = tempfile::NamedTempFile::new_in(parent)?;
    let (file, temporary_path) = temporary.into_parts();
    let mut file = tokio::fs::File::from_std(file);
    let mut stream = body.into_data_stream();
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| AppError::UploadError(error.to_string()))?;
        size = size
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| AppError::UploadError("tool artifact is too large".into()))?;
        if size > MAX_TOOL_ARTIFACT_BYTES {
            return Err(AppError::UploadError(format!(
                "tool artifact exceeds the {} byte limit",
                MAX_TOOL_ARTIFACT_BYTES
            )));
        }
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
    }
    if size == 0 {
        return Err(AppError::BadRequest("tool artifact cannot be empty".into()));
    }
    file.sync_all().await?;
    drop(file);

    let actual_digest = encode_digest(hasher.finalize());
    if actual_digest != expected_digest {
        return Err(AppError::BadRequest(format!(
            "tool artifact digest mismatch: expected {expected_digest}, received {actual_digest}"
        )));
    }

    let temporary_file: &FsPath = temporary_path.as_ref();
    match tokio::fs::hard_link(temporary_file, &destination).await {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let (existing_digest, existing_size) = sha256_file(&destination).await?;
            if existing_digest == expected_digest && existing_size == size {
                Ok(false)
            } else {
                Err(AppError::Conflict(format!(
                    "immutable tool artifact '{}' already exists with different content",
                    destination.display()
                )))
            }
        }
        Err(error) => Err(error.into()),
    }
}

async fn sha256_file(path: &FsPath) -> AppResult<(String, u64)> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size += read as u64;
    }
    Ok((encode_digest(hasher.finalize()), size))
}

fn encode_digest(digest: impl AsRef<[u8]>) -> String {
    use std::fmt::Write as _;

    digest
        .as_ref()
        .iter()
        .fold(String::with_capacity(64), |mut encoded, byte| {
            write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
            encoded
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_test::TestServer;
    use vm_packages::tool_artifact_path;

    use crate::config::Config;
    use crate::registry::{NpmRegistry, PypiRegistry};
    use crate::upstream::UpstreamClient;

    fn state(root: &FsPath) -> Arc<AppState> {
        let mut config = Config::default();
        config.security.require_authentication = true;
        config.security.read_keys = vec!["read".into()];
        config.security.publish_keys = vec!["publish".into()];
        Arc::new(AppState {
            data_dir: root.to_path_buf(),
            server_addr: "http://localhost:3080".into(),
            upstream_client: Arc::new(UpstreamClient::disabled()),
            config: Arc::new(config),
            npm_registry: NpmRegistry::new(),
            pypi_registry: PypiRegistry::new(),
        })
    }

    #[tokio::test]
    async fn artifact_upload_is_authenticated_verified_immutable_and_streamed_back() {
        let directory = tempfile::tempdir().unwrap();
        let content: &[u8] = b"one whole skills repository";
        let digest = encode_digest(Sha256::digest(content));
        let path = tool_artifact_path("agent-skills", "1.0.0", "any", &digest);
        let server = TestServer::new(router().with_state(state(directory.path())));

        assert_eq!(
            server.put(&path).bytes(content.into()).await.status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            server
                .put(&path)
                .add_header(header::AUTHORIZATION, "Bearer publish")
                .bytes(content.into())
                .await
                .status_code(),
            StatusCode::CREATED
        );
        assert_eq!(
            server
                .put(&path)
                .add_header(header::AUTHORIZATION, "Bearer publish")
                .bytes(content.into())
                .await
                .status_code(),
            StatusCode::OK
        );
        assert_eq!(
            server
                .get(&path)
                .add_header(header::AUTHORIZATION, "Bearer read")
                .await
                .as_bytes(),
            content
        );

        let bad_path = tool_artifact_path("agent-skills", "1.0.1", "any", &"f".repeat(64));
        assert_eq!(
            server
                .put(&bad_path)
                .add_header(header::AUTHORIZATION, "Bearer publish")
                .bytes(content.into())
                .await
                .status_code(),
            StatusCode::BAD_REQUEST
        );
    }
}
