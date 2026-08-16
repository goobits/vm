//! Capability-scoped dispatch for commands registered by a managed guest.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures::StreamExt;
use reqwest::header::CONTENT_TYPE;
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use url::Url;
use vm_core::{vm_print, vm_progress};

use super::command_context::managed_guest_context;
use super::managed_guest::{
    GuestRemoteCommands as Registry, RemoteCommandRegistration as Registration,
    GUEST_REMOTE_COMMANDS_PATH, REMOTE_COMMAND_SCHEMA,
};
use crate::error::{VmError, VmResult};

const MAX_CONFIG_BYTES: u64 = 64 * 1024;
const MAX_ARGUMENTS: usize = 128;
const MAX_ARGUMENT_BYTES: usize = 64 * 1024;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Serialize)]
struct RemoteRequest<'a> {
    schema: u8,
    arguments: &'a [String],
    idempotency_key: String,
}

#[derive(Deserialize)]
struct RemoteResponse {
    schema: u8,
    exit_code: u8,
    #[serde(default)]
    stdout: String,
    #[serde(default)]
    stderr: String,
}

pub(crate) async fn handle(raw_arguments: Vec<OsString>) -> VmResult<()> {
    let arguments = utf8_arguments(raw_arguments)?;
    let namespace = arguments.first().ok_or_else(|| unknown_command(""))?;

    if !managed_guest_context() {
        return Err(unknown_command(namespace));
    }

    let registry = load_registry(&registry_path())?;
    let registration = registry
        .commands
        .get(namespace)
        .ok_or_else(|| unregistered_command(namespace))?;
    validate_arguments(&arguments[1..], &registration.repair_command)?;
    let url = command_url(namespace, registration)?;
    let request = RemoteRequest {
        schema: REMOTE_COMMAND_SCHEMA,
        arguments: &arguments[1..],
        idempotency_key: uuid::Uuid::new_v4().to_string(),
    };
    let client = reqwest::Client::builder()
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| service_error(namespace, registration, error))?;
    let response = client
        .post(url)
        .bearer_auth(&registration.capability)
        .header(reqwest::header::ACCEPT, "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|error| service_error(namespace, registration, error))?;
    let status = response.status();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim();
    if content_type != "application/json" {
        return Err(remote_error(
            namespace,
            "service returned a non-JSON response",
            &registration.repair_command,
        ));
    }
    let body = limited_body(response, namespace, registration).await?;
    if !status.is_success() {
        return Err(remote_error(
            namespace,
            format!("service returned HTTP {}", status.as_u16()),
            &registration.repair_command,
        ));
    }
    let response: RemoteResponse = serde_json::from_slice(&body).map_err(|error| {
        remote_error(
            namespace,
            format!("service returned invalid JSON: {error}"),
            &registration.repair_command,
        )
    })?;
    render_response(namespace, response, registration)
}

fn registry_path() -> PathBuf {
    if std::env::var_os("VM_TEST_MODE").is_some() {
        if let Some(path) = std::env::var_os("VM_REMOTE_COMMANDS_FILE") {
            return path.into();
        }
    }
    PathBuf::from(GUEST_REMOTE_COMMANDS_PATH)
}

fn load_registry(path: &Path) -> VmResult<Registry> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        VmError::validation(
            format!("Remote command registrations are unavailable: {error}"),
            Some("Run on the controller host: vm start"),
        )
    })?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_CONFIG_BYTES {
        return Err(invalid_registry(
            "registration file must be a regular file no larger than 64 KiB",
        ));
    }
    validate_registry_owner(&metadata)?;
    let content = fs::read(path).map_err(|error| {
        VmError::validation(
            format!("Remote command registrations cannot be read: {error}"),
            Some("Run on the controller host: vm start"),
        )
    })?;
    let registry: Registry = serde_json::from_slice(&content)
        .map_err(|error| invalid_registry(format!("invalid JSON: {error}")))?;
    validate_registry(&registry)?;
    Ok(registry)
}

pub(crate) fn validate_registry(registry: &Registry) -> VmResult<()> {
    if registry.schema != REMOTE_COMMAND_SCHEMA {
        return Err(invalid_registry(format!(
            "unsupported schema {}",
            registry.schema
        )));
    }
    for (namespace, registration) in &registry.commands {
        validate_registration(namespace, registration)?;
    }
    Ok(())
}

#[cfg(unix)]
fn validate_registry_owner(metadata: &fs::Metadata) -> VmResult<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    if std::env::var_os("VM_TEST_MODE").is_none()
        && (metadata.uid() != 0 || metadata.permissions().mode() & 0o022 != 0)
    {
        return Err(invalid_registry(
            "registration file must be root-owned and not group/world writable",
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_registry_owner(_: &fs::Metadata) -> VmResult<()> {
    Ok(())
}

fn validate_registration(namespace: &str, registration: &Registration) -> VmResult<()> {
    if !valid_namespace(namespace) {
        return Err(invalid_registry(format!(
            "invalid command namespace '{namespace}'"
        )));
    }
    if registration.capability.is_empty()
        || registration.capability.len() > 4096
        || registration.capability.chars().any(char::is_control)
    {
        return Err(invalid_registry(format!(
            "invalid capability for '{namespace}'"
        )));
    }
    if registration.repair_command.len() > 512
        || !registration.repair_command.starts_with("vm ")
        || registration.repair_command.chars().any(char::is_control)
    {
        return Err(invalid_registry(format!(
            "invalid repair command for '{namespace}'"
        )));
    }
    command_url(namespace, registration).map(|_| ())
}

fn command_url(namespace: &str, registration: &Registration) -> VmResult<Url> {
    let mut url = Url::parse(&registration.endpoint).map_err(|error| {
        invalid_registry(format!("invalid endpoint for '{namespace}': {error}"))
    })?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid_registry(format!(
            "endpoint for '{namespace}' must be an HTTP(S) base URL without credentials, query, or fragment"
        )));
    }
    url.path_segments_mut()
        .map_err(|_| invalid_registry(format!("invalid endpoint for '{namespace}'")))?
        .pop_if_empty()
        .extend(["v1", "commands", namespace]);
    Ok(url)
}

fn valid_namespace(namespace: &str) -> bool {
    let mut characters = namespace.chars();
    namespace.len() <= 32
        && characters
            .next()
            .is_some_and(|character| character.is_ascii_lowercase())
        && characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

fn utf8_arguments(arguments: Vec<OsString>) -> VmResult<Vec<String>> {
    arguments
        .into_iter()
        .map(|argument| {
            argument.into_string().map_err(|_| {
                VmError::validation(
                    "Registered remote commands require UTF-8 arguments",
                    Some("Run: vm --help"),
                )
            })
        })
        .collect()
}

fn validate_arguments(arguments: &[String], repair_command: &str) -> VmResult<()> {
    let bytes = arguments.iter().map(String::len).sum::<usize>();
    if arguments.len() > MAX_ARGUMENTS || bytes > MAX_ARGUMENT_BYTES {
        return Err(VmError::validation(
            "Remote command arguments exceed the supported limit",
            Some(format!("Run: {repair_command}")),
        ));
    }
    Ok(())
}

async fn limited_body(
    response: reqwest::Response,
    namespace: &str,
    registration: &Registration,
) -> VmResult<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(remote_error(
            namespace,
            "service response exceeds 1 MiB",
            &registration.repair_command,
        ));
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| service_error(namespace, registration, error))?;
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(remote_error(
                namespace,
                "service response exceeds 1 MiB",
                &registration.repair_command,
            ));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn render_response(
    namespace: &str,
    response: RemoteResponse,
    registration: &Registration,
) -> VmResult<()> {
    if response.schema != REMOTE_COMMAND_SCHEMA || response.exit_code > 125 {
        return Err(remote_error(
            namespace,
            "service returned an unsupported response",
            &registration.repair_command,
        ));
    }
    if !response.stdout.is_empty() {
        vm_print!("{}", response.stdout);
    }
    if response.exit_code == 0 {
        if !response.stderr.is_empty() {
            vm_progress!("{}", response.stderr.trim_end());
        }
        return Ok(());
    }
    let detail = response.stderr.trim();
    Err(remote_error(
        namespace,
        if detail.is_empty() {
            format!("service exited with status {}", response.exit_code)
        } else {
            format!(
                "service exited with status {}: {detail}",
                response.exit_code
            )
        },
        &registration.repair_command,
    ))
}

fn unknown_command(namespace: &str) -> VmError {
    VmError::validation(
        format!("Unknown command '{namespace}'"),
        Some("Run: vm --help"),
    )
}

fn unregistered_command(namespace: &str) -> VmError {
    VmError::validation(
        format!("Remote command '{namespace}' is not registered in this environment"),
        Some("Run on the controller host: vm start"),
    )
}

fn invalid_registry(message: impl Into<String>) -> VmError {
    VmError::validation(
        format!("Remote command registration is invalid: {}", message.into()),
        Some("Run on the controller host: vm start"),
    )
}

fn service_error(
    namespace: &str,
    registration: &Registration,
    error: impl std::fmt::Display,
) -> VmError {
    remote_error(
        namespace,
        format!("service request failed: {error}"),
        &registration.repair_command,
    )
}

fn remote_error(namespace: &str, message: impl std::fmt::Display, repair_command: &str) -> VmError {
    VmError::validation(
        format!("Remote command '{namespace}' failed: {message}"),
        Some(format!("Run: {repair_command}")),
    )
}
