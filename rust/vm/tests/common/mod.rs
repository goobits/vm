// Common test utilities and fixtures
use anyhow::Result;
use std::path::PathBuf;

/// Resolve the path to the `vm` binary for integration testing.
///
/// This function tries multiple sources in order:
/// 1. `CARGO_BIN_EXE_vm` environment variable (set by `cargo test`)
/// 2. Fallback to `/workspace/.build/target/debug/vm` (legacy path)
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

    // Second try: fallback to legacy build path
    let fallback = PathBuf::from("/workspace/.build/target/debug/vm");
    if fallback.exists() {
        return Ok(fallback);
    }

    // Third try: relative path from workspace root
    let workspace_fallback = PathBuf::from("../target/debug/vm");
    if workspace_fallback.exists() {
        return Ok(workspace_fallback);
    }

    // Fourth try: absolute path to workspace target
    let absolute_fallback = PathBuf::from("/workspace/rust/target/debug/vm");
    if absolute_fallback.exists() {
        return Ok(absolute_fallback);
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
