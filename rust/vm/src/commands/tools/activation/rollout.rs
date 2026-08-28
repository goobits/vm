use std::time::Duration;

use futures_util::{stream, StreamExt};
use vm_config::GlobalConfig;
use vm_core::vm_warning;
use vm_packages::{
    ClaimToolActivationRequest, FinishToolActivationRequest, PlanToolActivationRequest,
    ToolActivationRecord, ToolActivationTargetPlan, ToolActivationTargetState,
    UpdateToolActivationTargetRequest,
};
use vm_provider::InstanceInfo;

use crate::cli::FleetArgs;
use crate::commands::command_context::load_runtime_subject_for_instance;
use crate::commands::packages::tooling;
use crate::commands::vm_ops::{self, InstanceStateFilter};
use crate::error::{VmError, VmResult};

use super::super::{reconcile::reconcile_subject, updates};
use super::worker::worker_id;

const TARGET_RETRY_INTERVAL: Duration = Duration::from_secs(2);
const TARGET_RETRY_TIMEOUT: Duration = Duration::from_secs(2 * 60);
const WORKER_LEASE_SECONDS: u64 = 5 * 60;
const MAX_CONCURRENT_TARGETS: usize = 4;

pub(in crate::commands) async fn repair() -> VmResult<usize> {
    super::worker::ensure_worker()?;
    let client = tooling::client()?;
    let repaired = client.repair_tool_activations().await?;
    reconcile_running_environments().await?;
    Ok(repaired)
}

pub(in crate::commands) async fn activate_deferred(
    provider: &str,
    environment: &str,
) -> VmResult<()> {
    let client = tooling::client()?;
    let activations = client.tool_activations().await?;
    for activation in activations {
        let Some(target) = activation.targets.iter().find(|target| {
            target.provider == provider
                && target.environment == environment
                && target.state == ToolActivationTargetState::Deferred
        }) else {
            continue;
        };
        let worker = worker_id()?;
        let Some(claimed) = client
            .claim_tool_activation(
                &activation.activation_id,
                &ClaimToolActivationRequest {
                    worker: worker.clone(),
                    lease_seconds: WORKER_LEASE_SECONDS,
                },
            )
            .await?
        else {
            continue;
        };
        activate_target(&client, &claimed, target.target_id.as_str(), &worker).await?;
        let finished = finish(&client, &claimed, &worker).await?;
        if let Some(failed) = finished.targets.iter().find(|candidate| {
            candidate.target_id == target.target_id
                && candidate.state == ToolActivationTargetState::Failed
        }) {
            return Err(VmError::validation(
                format!(
                    "Tool '{}' activation failed in '{}': {}",
                    finished.tool,
                    environment,
                    failed.error.as_deref().unwrap_or("unknown error")
                ),
                Some("Run `vm packages doctor --fix` on the controller and retry the start"),
            ));
        }
    }
    Ok(())
}

pub(super) async fn process_next() -> VmResult<bool> {
    let worker = worker_id()?;
    let client = tooling::client()?;
    let Some(mut activation) = client
        .claim_next_tool_activation(&ClaimToolActivationRequest {
            worker: worker.clone(),
            lease_seconds: WORKER_LEASE_SECONDS,
        })
        .await?
    else {
        return Ok(false);
    };
    if activation.targets.is_empty() {
        activation = plan(&client, activation, &worker).await?;
    }
    let pending = activation
        .targets
        .iter()
        .filter(|target| target.state == ToolActivationTargetState::Pending)
        .map(|target| target.target_id.clone())
        .collect::<Vec<_>>();
    for targets in pending.chunks(MAX_CONCURRENT_TARGETS) {
        let Some(renewed) = client
            .claim_tool_activation(
                &activation.activation_id,
                &ClaimToolActivationRequest {
                    worker: worker.clone(),
                    lease_seconds: WORKER_LEASE_SECONDS,
                },
            )
            .await?
        else {
            return Err(VmError::validation(
                "Tool activation lease was lost",
                Some("The host worker will retry after the current lease expires"),
            ));
        };
        activation = renewed;
        let results = collect_bounded(
            targets.iter().cloned(),
            MAX_CONCURRENT_TARGETS,
            |target_id| {
                let client = client.clone();
                let activation = activation.clone();
                let worker = worker.clone();
                async move { activate_target(&client, &activation, &target_id, &worker).await }
            },
        )
        .await;
        for result in results {
            result?;
        }
    }
    finish(&client, &activation, &worker).await?;
    Ok(true)
}

async fn collect_bounded<I, F, Fut, Output>(
    items: I,
    max_concurrent: usize,
    operation: F,
) -> Vec<Output>
where
    I: IntoIterator,
    F: FnMut(I::Item) -> Fut,
    Fut: std::future::Future<Output = Output>,
{
    stream::iter(items.into_iter().map(operation))
        .buffer_unordered(max_concurrent)
        .collect()
        .await
}

async fn plan(
    client: &vm_packages::PackageInfrastructureClient,
    activation: ToolActivationRecord,
    worker: &str,
) -> VmResult<ToolActivationRecord> {
    let global = GlobalConfig::load()?;
    let mut targets = if global.tools.contains_key(&activation.tool) {
        vm_ops::resolve_fleet_targets(
            &FleetArgs {
                fleet: true,
                provider: None,
                pattern: None,
            },
            InstanceStateFilter::Any,
        )?
    } else {
        Vec::new()
    };
    targets
        .sort_by(|left, right| (&left.provider, &left.name).cmp(&(&right.provider, &right.name)));
    let targets = targets
        .into_iter()
        .map(|instance| ToolActivationTargetPlan {
            target_id: target_id(&instance.provider, &instance.name),
            environment: instance.name,
            provider: instance.provider,
            initially_running: vm_ops::is_running_status(&instance.status),
        })
        .collect();
    client
        .plan_tool_activation(
            &activation.activation_id,
            &PlanToolActivationRequest {
                worker: worker.to_string(),
                targets,
                idempotency_key: format!("plan-{}", activation.activation_id),
            },
        )
        .await
        .map_err(VmError::from)
}

async fn activate_target(
    client: &vm_packages::PackageInfrastructureClient,
    activation: &ToolActivationRecord,
    target_id: &str,
    worker: &str,
) -> VmResult<()> {
    let target = activation
        .targets
        .iter()
        .find(|target| target.target_id == target_id)
        .ok_or_else(|| VmError::validation("Tool activation target is missing", None::<String>))?;
    let deadline = tokio::time::Instant::now() + TARGET_RETRY_TIMEOUT;
    let mut last_error = None;
    while tokio::time::Instant::now() < deadline {
        match activate_environment(&activation.tool, &target.provider, &target.environment).await {
            Ok(()) => {
                client
                    .update_tool_activation_target(
                        &activation.activation_id,
                        target_id,
                        &UpdateToolActivationTargetRequest {
                            worker: worker.to_string(),
                            state: ToolActivationTargetState::Active,
                            error: None,
                            idempotency_key: target_update_key(activation, target_id, "active"),
                        },
                    )
                    .await?;
                return Ok(());
            }
            Err(error) => last_error = Some(error),
        }
        tokio::time::sleep(TARGET_RETRY_INTERVAL).await;
    }
    let error = last_error
        .map(|error| error.to_string())
        .unwrap_or_else(|| "tool activation timed out".into());
    client
        .update_tool_activation_target(
            &activation.activation_id,
            target_id,
            &UpdateToolActivationTargetRequest {
                worker: worker.to_string(),
                state: ToolActivationTargetState::Failed,
                error: Some(error),
                idempotency_key: target_update_key(activation, target_id, "failed"),
            },
        )
        .await?;
    Ok(())
}

async fn activate_environment(tool: &str, provider: &str, environment: &str) -> VmResult<()> {
    let instance = InstanceInfo {
        name: environment.to_string(),
        id: String::new(),
        status: "planned".into(),
        provider: provider.to_string(),
        project: None,
        uptime: None,
        created_at: None,
    };
    let mut subject = load_runtime_subject_for_instance(None, None, &instance)?;
    updates::activate_tool(&mut subject, tool).await
}

async fn finish(
    client: &vm_packages::PackageInfrastructureClient,
    activation: &ToolActivationRecord,
    worker: &str,
) -> VmResult<ToolActivationRecord> {
    let current = client
        .tool_activation_for_release(&activation.release_id)
        .await?;
    let state = current
        .targets
        .iter()
        .map(|target| format!("{}:{:?}", target.target_id, target.state))
        .collect::<Vec<_>>()
        .join("\0");
    let revision = &vm_packages::sha256_hex(state)[..16];
    client
        .finish_tool_activation(
            &activation.activation_id,
            &FinishToolActivationRequest {
                worker: worker.to_string(),
                idempotency_key: format!("finish-{}-{revision}", activation.activation_id),
            },
        )
        .await
        .map_err(VmError::from)
}

fn target_update_key(activation: &ToolActivationRecord, target_id: &str, outcome: &str) -> String {
    let attempt = activation
        .targets
        .iter()
        .find(|target| target.target_id == target_id)
        .map_or(1, |target| target.attempts.saturating_add(1));
    format!(
        "{outcome}-{}-{target_id}-{attempt}",
        activation.activation_id
    )
}

async fn reconcile_running_environments() -> VmResult<()> {
    let instances = vm_ops::resolve_fleet_targets(
        &FleetArgs {
            fleet: true,
            provider: None,
            pattern: None,
        },
        InstanceStateFilter::Running,
    )?;
    for instance in instances {
        match load_runtime_subject_for_instance(None, None, &instance) {
            Ok(subject) => reconcile_subject(&subject).await?,
            Err(error) => vm_warning!("{}: {}", instance.name, error),
        }
    }
    Ok(())
}

fn target_id(provider: &str, environment: &str) -> String {
    let digest = vm_packages::sha256_hex(format!("{provider}\0{environment}"));
    format!("target-{}", &digest[..32])
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::{mpsc, Semaphore};

    fn activation(attempts: u32) -> ToolActivationRecord {
        ToolActivationRecord {
            activation_id: "activate-release".into(),
            release_id: "release".into(),
            checkout_id: "checkout".into(),
            tool: "typemill".into(),
            version: "1.2.0".into(),
            source_commit: "0123456789012345678901234567890123456789".into(),
            state: vm_packages::ToolActivationState::Activating,
            lease: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            targets: vec![vm_packages::ToolActivationTarget {
                target_id: "target-demo".into(),
                environment: "demo".into(),
                provider: "docker".into(),
                initially_running: true,
                state: ToolActivationTargetState::Pending,
                attempts,
                error: None,
                updated_at: chrono::Utc::now(),
            }],
        }
    }

    #[test]
    fn target_ids_are_stable_managed_components() {
        let target = target_id("docker", "typemill-dev");
        assert_eq!(target, target_id("docker", "typemill-dev"));
        assert!(vm_packages::validate_managed_id("target", &target).is_ok());
    }

    #[test]
    fn target_receipts_advance_with_each_attempt() {
        assert_eq!(
            target_update_key(&activation(2), "target-demo", "active"),
            "active-activate-release-target-demo-3"
        );
    }

    #[tokio::test]
    async fn bounded_target_execution_overlaps_four_and_queues_the_fifth() {
        let gate = std::sync::Arc::new(Semaphore::new(0));
        let (started_tx, mut started_rx) = mpsc::unbounded_channel();
        let task = tokio::spawn({
            let gate = std::sync::Arc::clone(&gate);
            async move {
                collect_bounded(0..5, MAX_CONCURRENT_TARGETS, move |target| {
                    let gate = std::sync::Arc::clone(&gate);
                    let started_tx = started_tx.clone();
                    async move {
                        started_tx.send(target).unwrap();
                        gate.acquire().await.unwrap().forget();
                        target
                    }
                })
                .await
            }
        });

        let mut first_batch = Vec::new();
        for _ in 0..MAX_CONCURRENT_TARGETS {
            first_batch.push(started_rx.recv().await.unwrap());
        }
        first_batch.sort_unstable();
        assert_eq!(first_batch, [0, 1, 2, 3]);
        assert!(started_rx.try_recv().is_err());

        gate.add_permits(1);
        let fifth = tokio::time::timeout(Duration::from_secs(1), started_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fifth, 4);

        gate.add_permits(MAX_CONCURRENT_TARGETS);
        let mut completed = task.await.unwrap();
        completed.sort_unstable();
        assert_eq!(completed, [0, 1, 2, 3, 4]);
    }
}
