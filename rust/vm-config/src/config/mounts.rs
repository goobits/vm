use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use vm_core::error::{Result, VmError};

/// Access granted to a guest for a host directory mount.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MountAccess {
    #[serde(rename = "read_only", alias = "ro")]
    ReadOnly,
    #[default]
    #[serde(rename = "read_write", alias = "rw")]
    ReadWrite,
}

impl MountAccess {
    pub fn is_read_write(access: &Self) -> bool {
        matches!(access, Self::ReadWrite)
    }

    pub const fn as_mode(self) -> &'static str {
        match self {
            Self::ReadOnly => "ro",
            Self::ReadWrite => "rw",
        }
    }
}

impl std::fmt::Display for MountAccess {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_mode())
    }
}

impl std::str::FromStr for MountAccess {
    type Err = VmError;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "ro" | "read_only" => Ok(Self::ReadOnly),
            "rw" | "read_write" => Ok(Self::ReadWrite),
            _ => Err(VmError::Config(format!(
                "Invalid mount access '{value}'. Use 'read_only' or 'read_write'"
            ))),
        }
    }
}

/// One additional host directory exposed to the guest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MountConfig {
    pub source: PathBuf,
    pub target: PathBuf,
    #[serde(default, skip_serializing_if = "MountAccess::is_read_write")]
    pub access: MountAccess,
}

impl MountConfig {
    /// Resolve a source relative to the project containing `vm.yaml`.
    pub fn resolved_source(&self, project_dir: &Path) -> Result<PathBuf> {
        resolve_mount_source(&self.source, project_dir)
    }
}

/// Canonicalize and reject host locations that should never be shared wholesale.
pub fn resolve_mount_source(source: &Path, project_dir: &Path) -> Result<PathBuf> {
    if source.as_os_str().is_empty() {
        return Err(VmError::Config("Mount source cannot be empty".to_string()));
    }

    let candidate = if source.is_absolute() {
        source.to_path_buf()
    } else {
        project_dir.join(source)
    };
    if !candidate.exists() {
        return Err(VmError::Config(format!(
            "Mount source does not exist: {}",
            candidate.display()
        )));
    }
    if !candidate.is_dir() {
        return Err(VmError::Config(format!(
            "Mount source is not a directory: {}",
            candidate.display()
        )));
    }

    let canonical = candidate.canonicalize().map_err(|error| {
        VmError::Config(format!(
            "Failed to resolve mount source '{}': {error}",
            candidate.display()
        ))
    })?;
    if is_dangerous_source(&canonical) {
        return Err(VmError::Config(format!(
            "Dangerous mount source is not allowed: {}",
            canonical.display()
        )));
    }
    Ok(canonical)
}

/// Validate a normalized absolute guest mount target.
pub fn validate_mount_target(target: &Path) -> Result<()> {
    let rendered = target.to_string_lossy();
    if !target.is_absolute()
        || target == Path::new("/")
        || rendered.ends_with('/')
        || rendered.contains("//")
        || target
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(VmError::Config(format!(
            "Mount target '{}' must be a normalized absolute path below /",
            target.display()
        )));
    }

    for reserved in [
        "/bin", "/boot", "/dev", "/etc", "/proc", "/root", "/sbin", "/sys", "/usr",
    ] {
        if target == Path::new(reserved) || target.starts_with(reserved) {
            return Err(VmError::Config(format!(
                "Mount target '{}' cannot replace a guest system filesystem",
                target.display()
            )));
        }
    }
    Ok(())
}

fn is_dangerous_source(path: &Path) -> bool {
    if ["/private/var/folders", "/private/var/tmp"]
        .iter()
        .any(|allowed| path.starts_with(allowed))
    {
        return false;
    }

    [
        "/",
        "/boot",
        "/dev",
        "/etc",
        "/proc",
        "/root",
        "/sbin",
        "/sys",
        "/usr",
        "/var",
        "/private/etc",
        "/private/var",
    ]
    .iter()
    .map(Path::new)
    .any(|dangerous| {
        path == dangerous || (dangerous != Path::new("/") && path.starts_with(dangerous))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_accepts_config_and_cli_spellings() {
        assert_eq!(
            "read_only".parse::<MountAccess>().unwrap(),
            MountAccess::ReadOnly
        );
        assert_eq!("ro".parse::<MountAccess>().unwrap(), MountAccess::ReadOnly);
        assert_eq!(
            "read_write".parse::<MountAccess>().unwrap(),
            MountAccess::ReadWrite
        );
        assert_eq!("rw".parse::<MountAccess>().unwrap(), MountAccess::ReadWrite);
    }

    #[test]
    fn relative_sources_resolve_from_project() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("shared");
        std::fs::create_dir(&source).unwrap();

        assert_eq!(
            resolve_mount_source(Path::new("shared"), root.path()).unwrap(),
            source.canonicalize().unwrap()
        );
    }

    #[test]
    fn targets_allow_application_roots_but_not_system_filesystems() {
        assert!(validate_mount_target(Path::new("/packages/auth")).is_ok());
        assert!(validate_mount_target(Path::new("/workspace")).is_ok());
        assert!(validate_mount_target(Path::new("/proc/keys")).is_err());
        assert!(validate_mount_target(Path::new("../relative")).is_err());
    }
}
