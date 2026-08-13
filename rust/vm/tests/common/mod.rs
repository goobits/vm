// Common test utilities and fixtures
use anyhow::Result;
use std::path::PathBuf;

/// Resolve the path to the `vm` binary for integration testing.
///
/// This function tries multiple sources in order:
/// 1. `CARGO_BIN_EXE_vm` environment variable (set by `cargo test`)
/// 2. Fallback to the configured machine-local Cargo target directory
///
/// If the binary cannot be found, returns an error with a helpful message.
pub fn binary_path() -> Result<PathBuf> {
    // First try: CARGO_BIN_EXE_vm (set by cargo test)
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_vm") {
        let path_buf = PathBuf::from(path);
        if path_buf.exists() {
            return Ok(path_buf);
        }
    }

    // Second try: the configured machine-local build path.
    let fallback = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(vm_core::MACHINE_CARGO_TARGET_DIR))
        .join("debug/vm");
    if fallback.exists() {
        return Ok(fallback);
    }

    anyhow::bail!(
        "vm binary not found\n\
         \n\
         Please build the binary first:\n\
           cd rust && cargo build --package vm\n\
         \n\
         Or set CARGO_BIN_EXE_vm to point to the binary"
    )
}
