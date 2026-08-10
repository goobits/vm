use std::{fs, path::PathBuf};

use vm_core::{vm_println, vm_success};
use vm_packages::{PackageEcosystem, RegisterPackage};

use crate::error::{VmError, VmResult};

use super::{configured_client, files::ApplianceFiles};

pub(super) async fn register(
    files: &ApplianceFiles,
    name: String,
    ecosystem: String,
    repository: String,
    default_branch: String,
) -> VmResult<()> {
    let ecosystem = ecosystem
        .parse::<PackageEcosystem>()
        .map_err(|error| VmError::validation(error.to_string(), None::<String>))?;
    let package = configured_client(files)?
        .register_package(&RegisterPackage {
            name,
            ecosystem,
            repository,
            default_branch,
        })
        .await?;
    vm_success!("Registered {} ({})", package.name, package.ecosystem);
    vm_println!("Repository: {}", package.repository);
    Ok(())
}

pub(super) async fn list(files: &ApplianceFiles) -> VmResult<()> {
    let packages = configured_client(files)?.package_definitions().await?;
    if packages.is_empty() {
        vm_println!("No shared packages are registered");
    }
    for package in packages {
        vm_println!(
            "{}\t{}\t{}#{}",
            package.name,
            package.ecosystem,
            package.repository,
            package.default_branch
        );
    }
    Ok(())
}

pub(super) async fn show(files: &ApplianceFiles, checkout_id: &str) -> VmResult<()> {
    let checkout = configured_client(files)?.checkout(checkout_id).await?;
    let json = serde_json::to_string_pretty(&checkout)
        .map_err(|error| VmError::general(error, "Failed to render checkout"))?;
    vm_println!("{json}");
    Ok(())
}

pub(super) fn configure_git_auth(
    files: &ApplianceFiles,
    token_file: Option<PathBuf>,
    clear: bool,
) -> VmResult<()> {
    let token = match (token_file, clear) {
        (Some(path), false) => fs::read_to_string(&path).map_err(|error| {
            VmError::filesystem(error, path.display().to_string(), "read Git token")
        })?,
        (None, true) => String::new(),
        _ => {
            return Err(VmError::validation(
                "Provide --token-file or --clear",
                None::<String>,
            ));
        }
    };
    files.set_git_token(token.trim())?;
    let message = if clear {
        "Package Git credential cleared"
    } else {
        "Package Git credential stored"
    };
    vm_success!("{message}");
    vm_println!("Run `vm packages up` to apply it to the appliance");
    Ok(())
}
