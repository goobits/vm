use std::io::{self, Write};
use vm_core::error::Result;

/// Simple confirmation prompt without external dependencies
pub fn confirm_prompt(message: &str) -> Result<bool> {
    print!("{message} [y/N]: ");
    io::stdout()
        .flush()
        .map_err(|e| vm_core::error::VmError::Internal(format!("Failed to flush stdout: {e}")))?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| vm_core::error::VmError::Internal(format!("Failed to read input: {e}")))?;

    let response = input.trim().to_lowercase();
    Ok(response == "y" || response == "yes")
}
