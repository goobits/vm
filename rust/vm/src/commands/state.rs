use std::path::PathBuf;

use dialoguer::Confirm;
use vm_config::AppConfig;
use vm_core::{vm_println, vm_progress, vm_success};
use vm_snapshot::{SnapshotManager, SnapshotMetadata, SnapshotScope};

use super::command_context::load_runtime_subject;
use crate::cli::SnapshotSubcommand;
use crate::error::{VmError, VmResult};

pub(super) async fn handle(
    command: SnapshotSubcommand,
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
    match command {
        SnapshotSubcommand::List { env } => {
            let target = snapshot_scope_target(config_path, profile, env)?;
            let snapshots = SnapshotManager::new()?.list_snapshots(Some(&target))?;
            for snapshot in snapshots {
                vm_println!(
                    "{}\t{}\t{} bytes",
                    snapshot.name,
                    snapshot.created_at,
                    snapshot.total_size_bytes
                );
            }
            Ok(())
        }
        SnapshotSubcommand::Show { name, env } => {
            let target = snapshot_scope_target(config_path, profile, env)?;
            let metadata = metadata(&target, &name)?;
            vm_println!("Name: {}", metadata.name);
            vm_println!("Environment: {}", metadata.project_name);
            vm_println!("Created: {}", metadata.created_at);
            vm_println!("Size: {} bytes", metadata.total_size_bytes);
            vm_println!("Services: {}", metadata.services.len());
            vm_println!("Volumes: {}", metadata.volumes.len());
            if let Some(description) = metadata.description {
                vm_println!("Description: {description}");
            }
            Ok(())
        }
        SnapshotSubcommand::Create {
            name,
            env,
            description,
            quiesce,
        } => {
            let subject = load_runtime_subject(config_path, profile, env)?;
            let provider = subject.provider.name().to_string();
            let target = subject.target;
            let config = AppConfig {
                global: subject.global_config,
                vm: subject.config,
            };
            vm_progress!("Creating snapshot '{name}' for '{target}'...");
            vm_snapshot::handle_create(
                &config,
                &provider,
                &name,
                description.as_deref(),
                quiesce,
                Some(&target),
                None,
                None,
                &[],
                false,
            )
            .await?;
            vm_success!("Created snapshot '{name}'");
            Ok(())
        }
        SnapshotSubcommand::Restore { name, env, yes } => {
            let subject = load_runtime_subject(config_path, profile, env)?;
            let provider = subject.provider.name().to_string();
            let target = subject.target;
            metadata(&target, &name)?;
            if !yes
                && !Confirm::new()
                    .with_prompt(format!("Restore snapshot '{name}' into '{target}'?"))
                    .default(false)
                    .interact()
                    .map_err(|error| {
                        VmError::general(error, "Failed to confirm snapshot restore")
                    })?
            {
                return Err(VmError::validation(
                    "Snapshot restore cancelled",
                    None::<String>,
                ));
            }
            let config = AppConfig {
                global: subject.global_config,
                vm: subject.config,
            };
            vm_snapshot::handle_restore(&config, &provider, &name, Some(&target), false).await?;
            vm_success!("Restored snapshot '{name}' into '{target}'");
            Ok(())
        }
        SnapshotSubcommand::Remove { name, env, yes } => {
            let target = snapshot_scope_target(config_path, profile, env)?;
            metadata(&target, &name)?;
            if !yes
                && !Confirm::new()
                    .with_prompt(format!("Remove snapshot '{name}' from '{target}'?"))
                    .default(false)
                    .interact()
                    .map_err(|error| {
                        VmError::general(error, "Failed to confirm snapshot removal")
                    })?
            {
                return Err(VmError::validation(
                    "Snapshot removal cancelled",
                    None::<String>,
                ));
            }
            SnapshotManager::new()?.delete_snapshot(SnapshotScope::Project(&target), &name)?;
            vm_success!("Removed snapshot '{name}'");
            Ok(())
        }
        SnapshotSubcommand::Export {
            name,
            env,
            output,
            compression,
        } => {
            let target = snapshot_scope_target(config_path.clone(), profile.clone(), env)?;
            let config = AppConfig::load(config_path, profile, None)?;
            let provider = config
                .vm
                .provider
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| config.global.container_provider().to_string());
            if output.exists() {
                return Err(VmError::validation(
                    format!("Export destination '{}' already exists", output.display()),
                    Some("Choose a new output path"),
                ));
            }
            vm_snapshot::handle_export(&provider, &name, Some(&output), compression, Some(&target))
                .await?;
            vm_success!("Exported snapshot '{name}' to {}", output.display());
            Ok(())
        }
        SnapshotSubcommand::Import { archive, name } => {
            let provider = vm_config::GlobalConfig::load()?
                .container_provider()
                .to_string();
            vm_snapshot::handle_import(&provider, &archive, Some(&name), false).await?;
            vm_success!("Imported snapshot '{name}'");
            Ok(())
        }
    }
}

fn snapshot_scope_target(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    env: Option<String>,
) -> VmResult<String> {
    match env {
        Some(name) => Ok(name),
        None => Ok(load_runtime_subject(config_path, profile, None)?.target),
    }
}

fn metadata(environment: &str, name: &str) -> VmResult<SnapshotMetadata> {
    let path = SnapshotManager::new()?
        .get_snapshot_dir(SnapshotScope::Project(environment), name)?
        .join("metadata.json");
    if !path.is_file() {
        return Err(VmError::validation(
            format!("Snapshot '{name}' was not found for '{environment}'"),
            None::<String>,
        ));
    }
    SnapshotMetadata::load(&path).map_err(VmError::from)
}
