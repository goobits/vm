use std::env;
use std::path::{Path, PathBuf};

/// Helper function to derive tool directory from a target directory path
fn derive_tool_dir_from_target(target: &Path) -> Option<PathBuf> {
    let parent = target.parent()?;

    // Case: .../rust/vm-config/target
    if parent.file_name() == Some(std::ffi::OsStr::new("vm-config")) {
        let rust = parent.parent()?;
        return rust.parent().map(|root| root.to_path_buf());
    }

    // Case: .../rust/target
    if parent.file_name() == Some(std::ffi::OsStr::new("rust")) {
        return parent.parent().map(|root| root.to_path_buf());
    }

    None
}

/// Get the VM tool installation directory
/// Priority order:
/// 1. VM_TOOL_DIR environment variable
/// 2. Directory containing the vm-config binary (../../ from binary)
/// 3. Current directory as fallback
pub fn get_tool_dir() -> PathBuf {
    // Check environment variable first - this should always work in tests
    if let Ok(tool_dir) = env::var("VM_TOOL_DIR") {
        return PathBuf::from(tool_dir);
    }

    // Try to find based on executable location, but don't fail if current_exe() fails
    if let Ok(mut exe_path) = env::current_exe() {
        // Resolve symlinks (important for installed binaries)
        if let Ok(canonical_path) = exe_path.canonicalize() {
            exe_path = canonical_path;
        }

        // Binaries are typically located at one of:
        // - VM_TOOL_DIR/rust/vm-config/target/(<platform>/)?{release,debug}/vm-config
        // - VM_TOOL_DIR/rust/target/(<platform>/)?{release,debug}/vm
        // We walk up until we find a directory named "target" (allowing for
        // an optional platform directory like "darwin-aarch64" between release/debug and target),
        // then detect whether we're under rust/vm-config/target or rust/target to derive VM_TOOL_DIR.
        if let Some(mut dir) = exe_path.parent() {
            // Search upwards up to a few levels for a directory named "target"
            let mut target_dir: Option<PathBuf> = None;
            for _ in 0..6 {
                if dir.file_name() == Some(std::ffi::OsStr::new("target")) {
                    target_dir = Some(dir.to_path_buf());
                    break;
                }
                if let Some(parent) = dir.parent() {
                    dir = parent;
                } else {
                    break;
                }
            }

            if let Some(target) = target_dir {
                if let Some(root) = derive_tool_dir_from_target(&target) {
                    return root;
                }
            }
        }
    }

    // Fallback to current directory - this should always work
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Get current user's UID
pub fn get_current_uid() -> u32 {
    #[cfg(unix)]
    {
        // SAFETY: This is safe because libc::getuid() is a pure system call that:
        // - Returns the real user ID of the calling process (always a valid u32)
        // - Takes no parameters that could cause undefined behavior
        // - Cannot fail or return an error code (always succeeds)
        // - Does not access or modify any memory beyond kernel space
        // - Is thread-safe and reentrant
        // - Maps directly to the getuid(2) POSIX system call
        //
        // Invariants:
        // - The returned UID is always valid for the current process
        // - No memory safety issues as no pointers are involved
        //
        // What could go wrong:
        // - Nothing; this is one of the safest system calls available
        // - The UID value itself is trustworthy as it comes from the kernel
        //
        // Safe alternative:
        // - No safe Rust alternative exists for this fundamental Unix operation
        // - This is the canonical way to get the current user ID in Unix systems
        unsafe { libc::getuid() }
    }
    #[cfg(not(unix))]
    {
        1000 // Default UID for non-Unix systems
    }
}

/// Get current user's GID
pub fn get_current_gid() -> u32 {
    #[cfg(unix)]
    {
        // SAFETY: This is safe because libc::getgid() is a pure system call that:
        // - Returns the real group ID of the calling process (always a valid u32)
        // - Takes no parameters that could cause undefined behavior
        // - Cannot fail or return an error code (always succeeds)
        // - Does not access or modify any memory beyond kernel space
        // - Is thread-safe and reentrant
        // - Maps directly to the getgid(2) POSIX system call
        //
        // Invariants:
        // - The returned GID is always valid for the current process
        // - No memory safety issues as no pointers are involved
        //
        // What could go wrong:
        // - Nothing; this is one of the safest system calls available
        // - The GID value itself is trustworthy as it comes from the kernel
        //
        // Safe alternative:
        // - No safe Rust alternative exists for this fundamental Unix operation
        // - This is the canonical way to get the current group ID in Unix systems
        unsafe { libc::getgid() }
    }
    #[cfg(not(unix))]
    {
        1000 // Default GID for non-Unix systems
    }
}

/// Get the config directory
/// Returns VM_TOOL_DIR/configs or ./configs
pub fn get_config_dir() -> PathBuf {
    let tool_dir = get_tool_dir();
    tool_dir.join("configs")
}

/// Get the presets directory
/// Returns VM_TOOL_DIR/configs/presets or ./configs/presets
pub fn get_presets_dir() -> PathBuf {
    get_config_dir().join("presets")
}

/// Get the schema file path
/// Returns VM_TOOL_DIR/vm.schema.yaml
pub fn get_schema_path() -> PathBuf {
    let tool_dir = get_tool_dir();
    tool_dir.join("vm.schema.yaml")
}

/// Get the default workspace path
/// Returns /home/USER/workspace on Unix or current directory
#[cfg(test)]
pub fn get_default_workspace_path() -> PathBuf {
    #[cfg(unix)]
    {
        if let Ok(home) = env::var("HOME") {
            return PathBuf::from(home).join("workspace");
        }
    }

    // Fallback to current directory
    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("workspace")
}

/// Resolve a path that might be relative to VM_TOOL_DIR
pub fn resolve_tool_path<P: AsRef<Path>>(path: P) -> PathBuf {
    let path = path.as_ref();

    if path.is_absolute() {
        path.to_path_buf()
    } else {
        let tool_dir = get_tool_dir();
        tool_dir.join(path)
    }
}
