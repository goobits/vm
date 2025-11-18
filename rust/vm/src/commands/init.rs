// Standard library imports
use std::path::PathBuf;

// External crate imports
use anyhow::Result;

/// Handles the `vm init` command by delegating to vm-config's init implementation.
/// This ensures consistency across all initialization paths.
pub fn handle_init(
    file: Option<PathBuf>,
    services: Option<String>,
    ports: Option<u16>,
    preset: Option<String>,
) -> Result<()> {
    // Delegate to vm-config's comprehensive init implementation
    // This eliminates code duplication and ensures all init paths
    // produce identical configurations
    vm_config::cli::init_config_file(file, services, ports, preset)
        .map_err(|e| anyhow::anyhow!("Initialization failed: {}", e))?;
    Ok(())
}
