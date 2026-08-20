use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
};

use vm_core::{vm_println, vm_success};
use vm_packages::{CheckoutRecord, PackageEcosystem, PackageIdentity, PackageInventory};

use crate::error::{VmError, VmResult};

use super::{appliance::configured_client, files::ApplianceFiles};

pub(super) async fn list(files: &ApplianceFiles) -> VmResult<()> {
    let client = configured_client(files)?;
    let (packages, inventory) = tokio::try_join!(client.package_definitions(), client.packages())?;
    if packages.is_empty() {
        vm_println!("No shared packages are registered");
        return Ok(());
    }
    vm_println!("NAME\tECOSYSTEM\tREGISTERED\tPUBLISHED\tINSTALLED\tCONSUMABLE\tSOURCE");
    for package in packages {
        let published = package_is_published(&inventory, package.ecosystem, &package.name);
        vm_println!(
            "{}\t{}\tyes\t{}\tn/a\t{}\t{}#{}",
            package.name,
            package.ecosystem,
            yes_no(published),
            yes_no(published),
            package.repository,
            package.default_branch
        );
    }
    Ok(())
}

fn package_is_published(
    inventory: &PackageInventory,
    ecosystem: PackageEcosystem,
    name: &str,
) -> bool {
    let registry = match ecosystem {
        PackageEcosystem::Npm => "npm",
        PackageEcosystem::Cargo => "cargo",
        PackageEcosystem::Python => "pypi",
    };
    let Ok(package) = PackageIdentity::new(ecosystem, name) else {
        return false;
    };
    inventory.get(registry).is_some_and(|packages| {
        packages
            .iter()
            .any(|candidate| package.matches_name(candidate))
    })
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

pub(super) async fn show(files: &ApplianceFiles, checkout_id: &str) -> VmResult<()> {
    let checkout = configured_client(files)?.checkout(checkout_id).await?;
    print_checkout(&checkout)
}

pub(super) async fn show_guest(checkout_id: &str) -> VmResult<()> {
    let checkout = super::runtime::GuestRuntime::discover()?
        .client()?
        .checkout(checkout_id)
        .await?;
    print_checkout(&checkout)
}

fn print_checkout(checkout: &CheckoutRecord) -> VmResult<()> {
    let json = serde_json::to_string_pretty(checkout)
        .map_err(|error| VmError::general(error, "Failed to render checkout"))?;
    vm_println!("{json}");
    Ok(())
}

pub(super) async fn status_guest() -> VmResult<()> {
    let healthy = async {
        let runtime = super::runtime::GuestRuntime::discover()?;
        let client = runtime.client()?;
        let (_packages, _tools) = tokio::try_join!(client.package_definitions(), client.tools())?;
        Ok::<_, VmError>(())
    }
    .await
    .is_ok();
    vm_println!(
        "Package infrastructure: {}",
        if healthy {
            "healthy"
        } else {
            "action required"
        }
    );
    Ok(())
}

pub(super) fn repair_github_credential(files: &ApplianceFiles) -> VmResult<bool> {
    if files.has_git_token()? {
        return Ok(false);
    }
    let Ok(token) = github_token() else {
        return Ok(false);
    };
    files.set_git_token(&token)?;
    vm_success!("Imported the active GitHub credential");
    Ok(true)
}

pub(super) fn configure_auth(
    files: &ApplianceFiles,
    git_token_file: Option<PathBuf>,
    github: bool,
    clear_git: bool,
) -> VmResult<()> {
    if git_token_file.is_none() && !github && !clear_git {
        return Err(VmError::validation(
            "Provide --github, a Git token file, or --clear",
            None::<String>,
        ));
    }
    let git_token = if github {
        Some(github_token()?)
    } else {
        credential(git_token_file, clear_git, "Git")?
    };
    if let Some(token) = git_token {
        files.set_git_token(&token)?;
        vm_success!("Package Git credential updated");
    }
    vm_println!("Run `vm packages up` to apply it to the appliance");
    Ok(())
}

fn github_token() -> VmResult<String> {
    let status = Command::new("gh")
        .args(["auth", "status", "--hostname", "github.com"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| VmError::general(error, "Could not run the GitHub CLI"))?;
    if !status.success() {
        return Err(invalid_github_credential());
    }
    let output = Command::new("gh")
        .args(["auth", "token", "--hostname", "github.com"])
        .output()
        .map_err(|error| VmError::general(error, "Could not run the GitHub CLI"))?;
    if !output.status.success() {
        return Err(invalid_github_credential());
    }
    let token = String::from_utf8(output.stdout)
        .map_err(|error| VmError::general(error, "GitHub CLI returned an invalid credential"))?
        .trim()
        .to_string();
    if token.is_empty() {
        return Err(VmError::validation(
            "The GitHub CLI returned an empty credential",
            Some("Run `gh auth login --hostname github.com`, then retry"),
        ));
    }
    Ok(token)
}

fn invalid_github_credential() -> VmError {
    VmError::validation(
        "The GitHub CLI has no valid active credential",
        Some("Run `gh auth login --hostname github.com`, then retry"),
    )
}

fn credential(path: Option<PathBuf>, clear: bool, kind: &str) -> VmResult<Option<String>> {
    match (path, clear) {
        (Some(path), false) => fs::read_to_string(&path)
            .map(|token| Some(token.trim().to_string()))
            .map_err(|error| {
                VmError::filesystem(
                    error,
                    path.display().to_string(),
                    format!("read {kind} token"),
                )
            }),
        (None, true) => Ok(Some(String::new())),
        (None, false) => Ok(None),
        (Some(_), true) => Err(VmError::validation(
            format!("Cannot set and clear the {kind} token together"),
            None::<String>,
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use vm_packages::PackageEcosystem;

    use super::package_is_published;

    #[test]
    fn publication_state_uses_native_registry_name_normalization() {
        let inventory = BTreeMap::from([
            ("npm".to_string(), vec!["@scope/shared".to_string()]),
            ("pypi".to_string(), vec!["shared_auth".to_string()]),
            ("cargo".to_string(), vec!["shared-core".to_string()]),
        ]);

        assert!(package_is_published(
            &inventory,
            PackageEcosystem::Npm,
            "@scope/shared"
        ));
        assert!(!package_is_published(
            &inventory,
            PackageEcosystem::Npm,
            "@scope/shared_other"
        ));
        assert!(package_is_published(
            &inventory,
            PackageEcosystem::Python,
            "shared-auth"
        ));
        assert!(package_is_published(
            &inventory,
            PackageEcosystem::Cargo,
            "shared_core"
        ));
        assert!(!package_is_published(
            &inventory,
            PackageEcosystem::Cargo,
            "unpublished"
        ));
    }
}
