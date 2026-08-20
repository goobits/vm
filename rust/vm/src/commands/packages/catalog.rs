use vm_core::vm_println;
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
