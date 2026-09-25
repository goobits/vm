//! Explicit host/guest copy endpoints and overwrite checks.

use std::path::{Path, PathBuf};

use vm_core::{vm_progress, vm_success};
use vm_provider::Provider;

use crate::error::{VmError, VmResult};

#[derive(Debug, PartialEq, Eq)]
enum Endpoint {
    Host(PathBuf),
    Guest(String),
}

fn endpoint(value: &str) -> VmResult<Endpoint> {
    if let Some(path) = value.strip_prefix("host:") {
        if path.is_empty() {
            return Err(VmError::validation("Empty host copy path", None::<String>));
        }
        return Ok(Endpoint::Host(PathBuf::from(path)));
    }
    if let Some(path) = value.strip_prefix("env:") {
        if path.is_empty() {
            return Err(VmError::validation(
                "Empty environment copy path",
                None::<String>,
            ));
        }
        return Ok(Endpoint::Guest(path.to_string()));
    }
    Err(VmError::validation(
        "Copy paths must begin with host: or env:",
        Some("Use `vm copy host:./file env:/workspace/file`"),
    ))
}

fn endpoint_pair(source: &str, destination: &str) -> VmResult<(Endpoint, Endpoint)> {
    let source = endpoint(source)?;
    let destination = endpoint(destination)?;
    if matches!(
        (&source, &destination),
        (Endpoint::Host(_), Endpoint::Guest(_)) | (Endpoint::Guest(_), Endpoint::Host(_))
    ) {
        Ok((source, destination))
    } else {
        Err(VmError::validation(
            "Copy requires exactly one host: endpoint and one env: endpoint",
            None::<String>,
        ))
    }
}

fn host_path(path: &Path) -> VmResult<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn exists(path: &Path) -> VmResult<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(VmError::from(error)),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum GuestPathState {
    Directory,
    Existing,
    Missing,
}

fn guest_path_state(provider: &dyn Provider, target: &str, path: &str) -> VmResult<GuestPathState> {
    let command = vec![
        "sh".to_string(),
        "-c".to_string(),
        "if [ -d \"$1\" ]; then exit 10; fi; if [ -e \"$1\" ] || [ -L \"$1\" ]; then exit 11; fi; if [ -d \"$(dirname \"$1\")\" ]; then exit 12; fi; exit 13".to_string(),
        "vm-copy".to_string(),
        path.to_string(),
    ];
    match provider.exec_status(Some(target), &command)?.code() {
        10 => Ok(GuestPathState::Directory),
        11 => Ok(GuestPathState::Existing),
        12 => Ok(GuestPathState::Missing),
        _ => Err(VmError::validation(
            format!("Cannot verify environment destination '{path}'"),
            Some("Check the destination and retry"),
        )),
    }
}

fn host_destination(path: &Path, source: &str) -> VmResult<PathBuf> {
    let path = host_path(path)?;
    if path.is_dir() {
        let basename = Path::new(source).file_name().ok_or_else(|| {
            VmError::validation("Source has no final path component", None::<String>)
        })?;
        Ok(path.join(basename))
    } else {
        Ok(path)
    }
}

fn nested_copy_path(path: &str, source: &Path) -> VmResult<String> {
    if path.ends_with("/.") || source.to_string_lossy().ends_with("/.") {
        return Err(VmError::validation(
            "Cannot verify a directory-content copy without --overwrite",
            Some("Use --overwrite to allow merging directory contents"),
        ));
    }
    let basename = source
        .file_name()
        .ok_or_else(|| VmError::validation("Source has no final path component", None::<String>))?;
    Ok(Path::new(path)
        .join(basename)
        .to_string_lossy()
        .into_owned())
}

pub fn handle_copy(
    provider: Box<dyn Provider>,
    target: &str,
    source: &str,
    destination: &str,
    overwrite: bool,
) -> VmResult<()> {
    let (source, destination) = endpoint_pair(source, destination)?;
    let (from, to) = match (source, destination) {
        (Endpoint::Host(source), Endpoint::Guest(destination)) => {
            let source = host_path(&source)?;
            if !exists(&source)? {
                return Err(VmError::validation(
                    format!("Host source '{}' does not exist", source.display()),
                    None::<String>,
                ));
            }
            let state = guest_path_state(provider.as_ref(), target, &destination)?;
            let destination = if state == GuestPathState::Directory
                && overwrite
                && source.to_string_lossy().ends_with("/.")
            {
                destination
            } else if state == GuestPathState::Directory {
                nested_copy_path(&destination, &source)?
            } else {
                destination
            };
            if !overwrite {
                let state = if state == GuestPathState::Directory {
                    guest_path_state(provider.as_ref(), target, &destination)?
                } else {
                    state
                };
                if state != GuestPathState::Missing {
                    return Err(VmError::conflict(
                        format!("Environment destination '{destination}' already exists"),
                        Some("Use --overwrite to replace it"),
                    ));
                }
            }
            (
                source.display().to_string(),
                format!("{target}:{destination}"),
            )
        }
        (Endpoint::Guest(source), Endpoint::Host(destination)) => {
            if !overwrite && source.ends_with("/.") {
                return Err(VmError::validation(
                    "Cannot verify a directory-content copy without --overwrite",
                    Some("Use --overwrite to allow merging directory contents"),
                ));
            }
            let destination = if overwrite && source.ends_with("/.") {
                host_path(&destination)?
            } else {
                host_destination(&destination, &source)?
            };
            if !overwrite && exists(&destination)? {
                return Err(VmError::conflict(
                    format!(
                        "Host destination '{}' already exists",
                        destination.display()
                    ),
                    Some("Use --overwrite to replace it"),
                ));
            }
            (
                format!("{target}:{source}"),
                destination.display().to_string(),
            )
        }
        _ => unreachable!("endpoint_pair checked direction"),
    };
    vm_progress!("Copying between host and '{target}'...");
    provider
        .copy(&from, &to, Some(target))
        .map_err(VmError::from)?;
    vm_success!("File copied");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{endpoint, endpoint_pair, Endpoint};
    use std::path::PathBuf;

    #[test]
    fn copy_requires_explicit_nonempty_endpoints() {
        assert_eq!(
            endpoint("host:./file").unwrap(),
            Endpoint::Host(PathBuf::from("./file"))
        );
        assert_eq!(
            endpoint("env:/tmp/file").unwrap(),
            Endpoint::Guest("/tmp/file".into())
        );
        for path in ["./file", "host:", "env:", "other:/tmp/file"] {
            assert!(endpoint(path).is_err());
        }
    }

    #[test]
    fn copy_requires_one_host_and_one_environment() {
        assert!(endpoint_pair("host:./a", "env:/a").is_ok());
        assert!(endpoint_pair("env:/a", "host:./a").is_ok());
        assert!(endpoint_pair("host:./a", "host:./b").is_err());
        assert!(endpoint_pair("env:/a", "env:/b").is_err());
    }
}
