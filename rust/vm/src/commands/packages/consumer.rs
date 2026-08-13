use std::collections::BTreeMap;

use vm_core::{vm_println, vm_success};
use vm_packages::RegisterConsumer;

use crate::cli::PackageConsumerSubcommand;
use crate::error::{VmError, VmResult};

use super::{appliance::configured_state_and_client, files::ApplianceFiles};

pub(super) async fn handle_catalog(
    files: &ApplianceFiles,
    command: PackageConsumerSubcommand,
) -> VmResult<()> {
    let (_, client) = configured_state_and_client(files)?;
    match command {
        PackageConsumerSubcommand::Register {
            name,
            repository,
            branch,
            dependencies,
        } => {
            let dependencies = dependencies
                .into_iter()
                .map(|dependency| parse_target(&dependency))
                .collect::<VmResult<BTreeMap<_, _>>>()?;
            let consumer = client
                .register_consumer(&RegisterConsumer {
                    name,
                    repository,
                    default_branch: branch,
                    dependencies,
                })
                .await?;
            vm_success!("Registered consumer {}", consumer.name);
        }
        PackageConsumerSubcommand::List => {
            let consumers = client.consumers().await?;
            if consumers.is_empty() {
                vm_println!("No package consumers are registered");
            }
            for consumer in consumers {
                let dependencies = consumer
                    .dependencies
                    .iter()
                    .map(|(package, version)| format!("{package}@{version}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                vm_println!(
                    "{}\t{}\t{}",
                    consumer.name,
                    dependencies,
                    consumer.repository
                );
            }
        }
    }
    Ok(())
}

pub(super) async fn show_consumers(files: &ApplianceFiles, package: &str) -> VmResult<()> {
    let (_, client) = configured_state_and_client(files)?;
    let consumers = client.package_consumers(package).await?;
    if consumers.is_empty() {
        vm_println!("No registered consumers use {package}");
    }
    for consumer in consumers {
        let pending = consumer
            .pending_version
            .map(|version| format!(" -> {version} pending"))
            .unwrap_or_default();
        vm_println!("{}\t{}{}", consumer.consumer, consumer.version, pending);
    }
    Ok(())
}

pub(super) async fn show_drift(files: &ApplianceFiles) -> VmResult<()> {
    let (_, client) = configured_state_and_client(files)?;
    for package in client.drift().await? {
        let latest = package.latest_version.as_deref().unwrap_or("unpublished");
        vm_println!("{}\tlatest {latest}", package.package);
        for consumer in package.consumers {
            let state = if consumer.version == latest {
                "current".to_string()
            } else if let Some(pending) = consumer.pending_version {
                format!("pending {pending}")
            } else {
                "drifted".to_string()
            };
            vm_println!("  {}\t{}\t{state}", consumer.consumer, consumer.version);
        }
    }
    Ok(())
}

fn parse_target(value: &str) -> VmResult<(String, String)> {
    let (package, version) = value.rsplit_once('@').ok_or_else(|| {
        VmError::validation(
            format!("Invalid package target '{value}'"),
            Some("Use package@version, for example auth@1.5.0"),
        )
    })?;
    if package.is_empty() || version.is_empty() {
        return Err(VmError::validation(
            format!("Invalid package target '{value}'"),
            Some("Use package@version, for example auth@1.5.0"),
        ));
    }
    Ok((package.to_string(), version.to_string()))
}

#[cfg(test)]
mod tests {
    use super::parse_target;

    #[test]
    fn parses_scoped_and_unscoped_package_targets() {
        assert_eq!(
            parse_target("auth@1.5.0").unwrap(),
            ("auth".into(), "1.5.0".into())
        );
        assert_eq!(
            parse_target("@scope/auth@1.5.0").unwrap(),
            ("@scope/auth".into(), "1.5.0".into())
        );
    }
}
