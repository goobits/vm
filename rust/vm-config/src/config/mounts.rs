use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
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
    // Targets are Unix guest paths, even when the controller runs on Windows.
    if !rendered.starts_with('/')
        || rendered.contains(['\\', '\0'])
        || rendered[1..]
            .split('/')
            .any(|component| matches!(component, "" | "." | ".."))
    {
        return Err(VmError::Config(format!(
            "Mount target '{}' must be a normalized absolute path below /",
            target.display()
        )));
    }

    for reserved in [
        "/bin", "/boot", "/dev", "/etc", "/proc", "/root", "/sbin", "/sys", "/usr",
    ] {
        if rendered == reserved || rendered.starts_with(&format!("{reserved}/")) {
            return Err(VmError::Config(format!(
                "Mount target '{}' cannot replace a guest system filesystem",
                target.display()
            )));
        }
    }
    Ok(())
}

fn is_dangerous_source(path: &Path) -> bool {
    if is_dangerous_windows_source(&path.to_string_lossy()) {
        return true;
    }
    #[cfg(windows)]
    for variable in [
        "SystemRoot",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramData",
    ] {
        if let Some(root) =
            std::env::var_os(variable).and_then(|root| PathBuf::from(root).canonicalize().ok())
        {
            let source = path.to_string_lossy().to_lowercase();
            let root = root.to_string_lossy().to_lowercase();
            if source == root || source.starts_with(&format!("{root}\\")) {
                return true;
            }
        }
    }
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

// Canonical Windows paths may have verbatim drive or UNC prefixes. Keep this
// lexical check host-independent so all CI platforms exercise the policy.
fn is_dangerous_windows_source(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_lowercase();
    let path = normalized.strip_prefix("//?/").unwrap_or(&normalized);
    let relative = if let Some(unc) = path
        .strip_prefix("unc/")
        .or_else(|| path.strip_prefix("//"))
    {
        if unc.starts_with("./") {
            return true; // Device namespaces are never ordinary shared directories.
        }
        unc.splitn(3, '/').nth(2).unwrap_or("")
    } else if path.as_bytes().get(1) == Some(&b':')
        && path.as_bytes()[0].is_ascii_alphabetic()
        && path.as_bytes().get(2) == Some(&b'/')
    {
        &path[3..]
    } else {
        return normalized.starts_with("//?/");
    };
    let relative = relative.trim_end_matches('/');
    let first = relative.split('/').next().unwrap_or("");
    relative.is_empty()
        || relative == "users"
        || matches!(
            first,
            "windows"
                | "program files"
                | "program files (x86)"
                | "programdata"
                | "recovery"
                | "system volume information"
                | "$recycle.bin"
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_sources_block_volume_and_system_roots() {
        for source in [
            r"C:\",
            "d:/",
            r"\\?\C:\",
            r"\\server\share",
            r"\\?\UNC\server\share\",
            r"\\.\C:\workspace",
            r"\\?\Volume{example}\workspace",
            r"C:\WINDOWS\System32",
            r"\\?\c:\Program Files\app",
            r"D:\ProgramData\app",
            r"C:\Users",
            r"C:\System Volume Information",
        ] {
            assert!(is_dangerous_windows_source(source), "{source}");
        }
        for source in [
            r"C:\Users\dev\project",
            r"C:\workspace",
            r"\\?\C:\workspace",
            r"\\server\share\project",
            r"\\?\UNC\server\share\project",
            r"C:\Windows-project",
            "/tmp/project",
        ] {
            assert!(!is_dangerous_windows_source(source), "{source}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn resolved_windows_volume_roots_are_rejected() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path().ancestors().last().unwrap();
        let error = resolve_mount_source(root, project.path()).unwrap_err();
        assert!(
            error.to_string().contains("Dangerous mount source"),
            "{error}"
        );
        assert!(resolve_mount_source(project.path(), project.path()).is_ok());
    }

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
        for invalid in [
            "/",
            "/packages/",
            "/packages//auth",
            "/packages/./auth",
            "/packages/../auth",
            "C:/packages",
            "/packages\\auth",
        ] {
            assert!(
                validate_mount_target(Path::new(invalid)).is_err(),
                "{invalid}"
            );
        }
        assert!(validate_mount_target(Path::new("/usr-local")).is_ok());
    }
}
