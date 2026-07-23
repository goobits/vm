//! Explicit maintenance for project-owned environment data.

use crate::error::VmResult;
use vm_core::{vm_println, vm_success};
use vm_provider::Provider;

pub fn prune_pnpm_store(provider: Box<dyn Provider>, container: Option<&str>) -> VmResult<()> {
    vm_println!(
        "Pruning the pnpm store in {}. Stop installs in environments sharing this store first.",
        target_name(container)
    );
    provider.exec(
        container,
        &["pnpm".to_string(), "store".to_string(), "prune".to_string()],
    )?;
    vm_success!("pnpm store prune completed");
    Ok(())
}

fn target_name(container: Option<&str>) -> &str {
    container.unwrap_or("the default environment")
}
