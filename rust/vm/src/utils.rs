//! Utility functions for the VM crate.

use crate::error::{VmError, VmResult};

// Password generation has been moved to vm_core::secrets module

/// VM-local wrapper around the shared core arrow-key confirmation prompt.
pub fn confirm_select(prompt: &str, default: bool) -> VmResult<bool> {
    vm_core::prompts::confirm_select(prompt, default)
        .map_err(|e| VmError::general(e, "Failed to read user selection"))
}
