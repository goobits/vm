use std::collections::BTreeMap;

use vm_core::{vm_println, vm_success};
use vm_packages::{
    CreateRollout, PackageDrift, PackageInfrastructureClient, RegisterConsumer, RolloutRecord,
    RolloutState,
};

use crate::cli::PackageConsumerSubcommand;
use crate::error::{VmError, VmResult};

use super::{appliance::configured_state_and_client, files::ApplianceFiles};

pub(super) async fn handle_catalog(
    files: &ApplianceFiles,
    command: PackageConsumerSubcommand,
) -> VmResult<()> {
    let (_, client) = configured_state_and_client(files)?;
    match command {
        PackageConsumerSubcommand::Retry { name } => {
            if !client
                .consumers()
                .await?
                .iter()
                .any(|consumer| consumer.name == name)
            {
                return Err(VmError::validation(
                    format!("Consumer '{name}' is not registered"),
                    Some("Run `vm packages consumers list` to see registered projects"),
                ));
            }
            let rollouts = client.rollouts().await?;
            let mut count = 0;
            for package in client.drift().await? {
                let Some(request) = retry_request(&package, &rollouts, &name) else {
                    continue;
                };
                let rollout = client.create_rollout(&request).await?;
                vm_success!(
                    "Queued {}@{} for {}",
                    rollout.package,
                    rollout.version,
                    rollout.consumer
                );
                count += 1;
            }
            if count == 0 {
                vm_println!("No failed dependency updates need retry for {name}");
            }
        }
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
        PackageConsumerSubcommand::List {
            package: Some(package),
        } => {
            return show_consumers(&client, &package).await;
        }
        PackageConsumerSubcommand::List { package: None } => {
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
        PackageConsumerSubcommand::Show { name } => {
            let consumer = client
                .consumers()
                .await?
                .into_iter()
                .find(|consumer| consumer.name == name)
                .ok_or_else(|| {
                    VmError::validation(
                        format!("Consumer '{name}' is not registered"),
                        Some("Run `vm packages consumers list` to see registered consumers"),
                    )
                })?;
            vm_println!("Name: {}", consumer.name);
            vm_println!("Repository: {}", consumer.repository);
            vm_println!("Branch: {}", consumer.default_branch);
            for (package, version) in consumer.dependencies {
                vm_println!("Dependency: {package}@{version}");
            }
        }
        PackageConsumerSubcommand::Drift { package } => {
            return show_drift(&client, package.as_deref()).await
        }
    }
    Ok(())
}

async fn show_consumers(client: &PackageInfrastructureClient, package: &str) -> VmResult<()> {
    let consumers = client.package_consumers(package).await?;
    if consumers.is_empty() {
        vm_println!("No registered consumers use {package}");
    }
    let rollouts = client.rollouts().await?;
    for consumer in consumers {
        let pending = consumer
            .pending_version
            .map(|version| format!(" -> {version} pending"))
            .unwrap_or_default();
        vm_println!("{}\t{}{}", consumer.consumer, consumer.version, pending);
        print_rollout(&rollouts, package, &consumer.consumer, &consumer.version);
    }
    Ok(())
}

async fn show_drift(
    client: &PackageInfrastructureClient,
    requested_package: Option<&str>,
) -> VmResult<()> {
    let rollouts = client.rollouts().await?;
    for package in client.drift().await? {
        if requested_package.is_some_and(|name| name != package.package) {
            continue;
        }
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
            print_rollout(
                &rollouts,
                &package.package,
                &consumer.consumer,
                &consumer.version,
            );
        }
    }
    Ok(())
}

fn latest_rollout<'a>(
    rollouts: &'a [RolloutRecord],
    package: &str,
    consumer: &str,
) -> Option<&'a RolloutRecord> {
    rollouts
        .iter()
        .filter(|rollout| rollout.package == package && rollout.consumer == consumer)
        .max_by_key(|rollout| rollout.created_at)
}

fn retry_request(
    package: &PackageDrift,
    rollouts: &[RolloutRecord],
    consumer: &str,
) -> Option<CreateRollout> {
    let version = package.latest_version.as_ref()?;
    let usage = package
        .consumers
        .iter()
        .find(|usage| usage.consumer == consumer)?;
    let failed = latest_rollout(rollouts, &package.package, consumer)?;
    if usage.version == *version
        || usage.pending_version.is_some()
        || failed.state != RolloutState::Failed
    {
        return None;
    }
    Some(CreateRollout {
        package: package.package.clone(),
        version: version.clone(),
        consumer: consumer.into(),
        actor: "package-controller".into(),
        // Repeating a retry request resumes the same attempt even if source
        // preparation or its HTTP response failed.
        idempotency_key: format!(
            "retry-rollout-{}",
            vm_packages::sha256_hex(format!("{}:{version}", failed.rollout_id))
        ),
    })
}

fn print_rollout(rollouts: &[RolloutRecord], package: &str, consumer: &str, current_version: &str) {
    let Some(rollout) = latest_rollout(rollouts, package, consumer) else {
        return;
    };
    if rollout.version == current_version || rollout.state == RolloutState::Closed {
        return;
    }
    match rollout.state {
        RolloutState::ReadyForReview => {
            if let Some(branch) = &rollout.branch {
                vm_println!(
                    "  Review {}@{} in branch {branch}",
                    package,
                    rollout.version
                );
            }
        }
        RolloutState::Failed => {
            vm_println!("  Update to {} failed; repair consumer checks, then run: vm packages consumers retry {}", rollout.version, consumer);
        }
        _ => {}
    }
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
    use super::*;

    #[test]
    fn failed_consumer_updates_have_an_idempotent_retry_without_republishing() {
        let now = chrono::Utc::now();
        let mut rollout = RolloutRecord {
            rollout_id: "rollout-failed".into(),
            package: "auth".into(),
            version: "1.1.0".into(),
            consumer: "project-a".into(),
            ecosystem: vm_packages::PackageEcosystem::Cargo,
            state: RolloutState::Failed,
            base_commit: None,
            branch: None,
            worktree: None,
            submitted_commit: None,
            created_at: now,
            updated_at: now,
            transitions: Vec::new(),
        };
        let mut drift = PackageDrift {
            package: "auth".into(),
            latest_version: Some("1.2.0".into()),
            consumers: vec![vm_packages::ConsumerUsage {
                consumer: "project-a".into(),
                version: "1.0.0".into(),
                pending_version: None,
                rollout_id: None,
            }],
        };
        let request = retry_request(&drift, &[rollout.clone()], "project-a").unwrap();
        assert_eq!(request.version, "1.2.0");
        assert_eq!(
            request,
            retry_request(&drift, &[rollout.clone()], "project-a").unwrap()
        );
        drift.consumers[0].pending_version = Some("1.2.0".into());
        assert!(retry_request(&drift, &[rollout.clone()], "project-a").is_none());
        drift.consumers[0].pending_version = None;
        drift.consumers[0].version = "1.2.0".into();
        assert!(retry_request(&drift, &[rollout.clone()], "project-a").is_none());
        drift.consumers[0].version = "1.0.0".into();
        rollout.state = RolloutState::ReadyForReview;
        assert!(retry_request(&drift, &[rollout], "project-a").is_none());
    }

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
