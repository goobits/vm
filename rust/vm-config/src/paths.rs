use std::path::{Path, PathBuf};
use std::env;

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
    if let Ok(exe_path) = env::current_exe() {
        // vm-config is at VM_TOOL_DIR/rust/vm-config/target/release/vm-config
        // or VM_TOOL_DIR/rust/target/release/vm-config
        // Go up to find the root
        if let Some(parent) = exe_path.parent() {
            // Check if we're in target/release or target/debug
            if parent.file_name() == Some(std::ffi::OsStr::new("release")) ||
               parent.file_name() == Some(std::ffi::OsStr::new("debug")) {
                // Go up to find VM_TOOL_DIR
                if let Some(target) = parent.parent() {
                    if target.file_name() == Some(std::ffi::OsStr::new("target")) {
                        // Could be either rust/vm-config/target or rust/target
                        if let Some(rust_or_vm_config) = target.parent() {
                            if rust_or_vm_config.file_name() == Some(std::ffi::OsStr::new("vm-config")) {
                                // We're in rust/vm-config/target, go up two more
                                if let Some(rust) = rust_or_vm_config.parent() {
                                    if let Some(root) = rust.parent() {
                                        return root.to_path_buf();
                                    }
                                }
                            } else if rust_or_vm_config.file_name() == Some(std::ffi::OsStr::new("rust")) {
                                // We're in rust/target, go up one more
                                if let Some(root) = rust_or_vm_config.parent() {
                                    return root.to_path_buf();
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Fallback to current directory - this should always work
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
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