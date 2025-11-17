use serde_json::json;
use sqlx::SqlitePool;
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};
use vm_config::config::VmConfig;
use vm_orchestrator::{Workspace, WorkspaceOrchestrator, WorkspaceStatus};
use vm_provider::get_provider;

pub async fn start_provisioner_task(pool: SqlitePool, interval_secs: u64) {
    let orchestrator = WorkspaceOrchestrator::new(pool);
    let mut interval = interval(Duration::from_secs(interval_secs));

    info!(
        "Provisioner task running (checks every {} seconds)",
        interval_secs
    );

    loop {
        interval.tick().await;

        if let Err(e) = process_pending_workspaces(&orchestrator).await {
            error!("Provisioner error: {}", e);
        }
    }
}

async fn process_pending_workspaces(orchestrator: &WorkspaceOrchestrator) -> anyhow::Result<()> {
    // Get all workspaces with status="creating"
    let creating = orchestrator
        .get_workspaces_by_status(WorkspaceStatus::Creating)
        .await?;

    for workspace in creating {
        info!(
            "Provisioning workspace: {} ({})",
            workspace.name, workspace.id
        );

        // Spawn blocking task since vm-provider is sync
        let workspace_clone = workspace.clone();
        let orchestrator_clone = orchestrator.clone();

        tokio::task::spawn(async move {
            // Find the create operation for this workspace
            let operation_id = orchestrator_clone
                .get_operations(
                    Some(workspace_clone.id.clone()),
                    Some(vm_orchestrator::operation::OperationType::Create),
                    None,
                )
                .await
                .ok()
                .and_then(|ops| ops.first().map(|op| op.id.clone()));

            // Update operation to running
            if let Some(ref op_id) = operation_id {
                let _ = orchestrator_clone
                    .update_operation_status(
                        op_id,
                        vm_orchestrator::operation::OperationStatus::Running,
                        None,
                    )
                    .await;
            }

            match provision_workspace(&workspace_clone).await {
                Ok((provider_id, connection_info)) => {
                    info!("Successfully provisioned workspace: {}", workspace_clone.id);

                    orchestrator_clone
                        .update_workspace_status(
                            &workspace_clone.id,
                            WorkspaceStatus::Running,
                            Some(provider_id),
                            Some(connection_info),
                            None,
                        )
                        .await
                        .ok();

                    // Update operation to success
                    if let Some(ref op_id) = operation_id {
                        let _ = orchestrator_clone
                            .update_operation_status(
                                op_id,
                                vm_orchestrator::operation::OperationStatus::Success,
                                None,
                            )
                            .await;
                    }
                }
                Err(e) => {
                    error!(
                        "Failed to provision workspace {}: {}",
                        workspace_clone.id, e
                    );

                    orchestrator_clone
                        .update_workspace_status(
                            &workspace_clone.id,
                            WorkspaceStatus::Failed,
                            None,
                            None,
                            Some(e.to_string()),
                        )
                        .await
                        .ok();

                    // Update operation to failed
                    if let Some(ref op_id) = operation_id {
                        let _ = orchestrator_clone
                            .update_operation_status(
                                op_id,
                                vm_orchestrator::operation::OperationStatus::Failed,
                                Some(e.to_string()),
                            )
                            .await;
                    }
                }
            }
        });
    }

    Ok(())
}

async fn provision_workspace(workspace: &Workspace) -> anyhow::Result<(String, serde_json::Value)> {
    // Build minimal VmConfig for this workspace
    let config = build_vm_config(workspace)?;
    let instance_name = workspace.name.clone();

    // Provision in blocking context (vm-provider is sync)
    // We create the provider inside spawn_blocking to avoid Send issues
    let instance_info = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        // Get provider (Docker/Tart/etc) inside blocking context
        let provider = get_provider(config)?;

        // Create with instance name
        provider.create_instance(&instance_name)?;

        // Get instance info
        let instances = provider.list_instances()?;
        let info = instances
            .into_iter()
            .find(|i| i.name == instance_name)
            .ok_or_else(|| anyhow::anyhow!("Instance not found after creation"))?;

        Ok(info)
    })
    .await??;

    // Extract connection info
    let connection_info = json!({
        "container_id": instance_info.id,
        "status": instance_info.status,
        "ssh_command": format!("vm ssh {}", workspace.name),
    });

    Ok((instance_info.id, connection_info))
}

fn build_vm_config(workspace: &Workspace) -> anyhow::Result<VmConfig> {
    // Build config from workspace metadata
    let mut config = VmConfig::default();

    // Set project name
    config.project = Some(vm_config::config::ProjectConfig {
        name: Some(workspace.name.clone()),
        ..Default::default()
    });

    // Apply template-based defaults
    // TODO: Use PresetDetector to load full preset configs once we have repo cloning
    // For now, apply basic template defaults manually
    if let Some(template) = &workspace.template {
        info!(
            "Applying template '{}' for workspace {}",
            template, workspace.name
        );
        apply_template_defaults(&mut config, template);
    }

    // Always set the provider
    config.provider = Some(workspace.provider.clone());

    Ok(config)
}

/// Apply template defaults to config
/// This is a simplified version. Full implementation should use vm_config::preset::PresetDetector
fn apply_template_defaults(config: &mut VmConfig, template: &str) {
    match template {
        "nodejs" | "node" => {
            if config.versions.is_none() {
                config.versions = Some(Default::default());
            }
            if let Some(ref mut versions) = config.versions {
                versions.node = Some("20".to_string());
            }
        }
        "python" | "py" => {
            if config.versions.is_none() {
                config.versions = Some(Default::default());
            }
            if let Some(ref mut versions) = config.versions {
                versions.python = Some("3.11".to_string());
            }
        }
        "rust" => {
            // Rust typically doesn't need version config (uses rustup)
        }
        _ => {
            warn!("Unknown template '{}', using defaults", template);
        }
    }
}
