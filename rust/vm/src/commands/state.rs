use std::path::PathBuf;

use dialoguer::Confirm;
use vm_config::AppConfig;
use vm_core::{vm_println, vm_progress, vm_success};
use vm_snapshot::{SnapshotManager, SnapshotMetadata, SnapshotScope};

use super::command_context::{load_runtime_subject, project_name, require_project_config};
use crate::cli::SnapshotSubcommand;
use crate::error::{VmError, VmResult};

pub(super) async fn handle(
    command: SnapshotSubcommand,
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
    match command {
        SnapshotSubcommand::List { env } => {
            let project = snapshot_project(config_path, profile)?;
            let snapshots = SnapshotManager::new()?.list_snapshots_in_scope(project.scope())?;
            for snapshot in snapshots.into_iter().filter(|snapshot| {
                env.as_deref().map_or(true, |selected| {
                    snapshot.source_environment.as_deref() == Some(selected)
                })
            }) {
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
            let project = snapshot_project(config_path, profile)?;
            let metadata = metadata(&project, &name)?;
            verify_environment_filter(&metadata, env.as_deref())?;
            vm_println!("Name: {}", metadata.name);
            vm_println!("Project: {}", metadata.project_name);
            if let Some(environment) = &metadata.source_environment {
                vm_println!("Environment: {environment}");
            }
            vm_println!("Provider: {}", metadata.provider);
            vm_println!("Architecture: {}", metadata.architecture);
            vm_println!("Consistency: {}", metadata.consistency);
            vm_println!("Created: {}", metadata.created_at);
            vm_println!("Size: {} bytes", metadata.total_size_bytes);
            vm_println!("Services: {}", metadata.services.len());
            vm_println!("Volumes: {}", metadata.volumes.len());
            if let Some(file) = &metadata.native_vm_file {
                vm_println!("Native VM archive: {file}");
            }
            for mount in &metadata.excluded_mounts {
                vm_println!(
                    "Excluded mount: {} {} -> {} ({})",
                    mount.service,
                    mount.source.as_deref().unwrap_or("<anonymous>"),
                    mount.target,
                    mount.kind
                );
            }
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
            let subject = load_runtime_subject(config_path.clone(), profile, env)?;
            let provider = subject.provider.name().to_string();
            ensure_snapshot_provider(&provider)?;
            require_project_config(&subject.config)?;
            let project = project_name(&subject.config).to_string();
            let project_dir = snapshot_project_dir(config_path.as_deref(), &subject)?;
            let target = subject.target;
            let config = AppConfig {
                global: subject.global_config,
                vm: subject.config,
            };
            vm_progress!("Creating snapshot '{name}' for '{target}'...");
            if provider == "tart" {
                let home = tart_home(&config.vm, &project)?;
                vm_snapshot::handle_tart_create(
                    &config.vm,
                    &name,
                    description.as_deref(),
                    quiesce,
                    &project,
                    &target,
                    &project_dir,
                    home.as_deref(),
                )
                .await?;
            } else {
                vm_snapshot::handle_create(
                    &config,
                    &provider,
                    &name,
                    description.as_deref(),
                    quiesce,
                    Some(&project),
                    Some(&target),
                    Some(&project_dir),
                    None,
                    None,
                    &[],
                    false,
                )
                .await?;
            }
            vm_success!("Created snapshot '{name}'");
            Ok(())
        }
        SnapshotSubcommand::Restore { name, env, yes } => {
            let subject = load_runtime_subject(config_path.clone(), profile, env)?;
            let provider = subject.provider.name().to_string();
            ensure_snapshot_provider(&provider)?;
            require_project_config(&subject.config)?;
            let project = project_name(&subject.config).to_string();
            let owner = subject
                .config
                .owning_config_path()
                .ok_or_else(|| {
                    VmError::validation(
                        "Snapshot restore requires a project configuration",
                        None::<String>,
                    )
                })?
                .to_path_buf();
            let project_dir = snapshot_project_dir(config_path.as_deref(), &subject)?;
            let target = subject.target;
            let snapshot = metadata(
                &ProjectSnapshotScope {
                    name: project.clone(),
                    config_path: owner.clone(),
                },
                &name,
            )?;
            verify_environment_filter(&snapshot, Some(&target))?;
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
            if provider == "tart" {
                let home = tart_home(&config.vm, &project)?;
                vm_snapshot::handle_tart_restore(&name, &project, &target, home.as_deref(), &owner)
                    .await?;
            } else {
                vm_snapshot::handle_restore(
                    &config,
                    &provider,
                    &name,
                    Some(&project),
                    Some(&target),
                    Some(&project_dir),
                    false,
                )
                .await?;
            }
            vm_success!("Restored snapshot '{name}' into '{target}'");
            Ok(())
        }
        SnapshotSubcommand::Remove { name, env, yes } => {
            let project = snapshot_project(config_path, profile)?;
            let snapshot = metadata(&project, &name)?;
            verify_environment_filter(&snapshot, env.as_deref())?;
            if !yes
                && !Confirm::new()
                    .with_prompt(format!(
                        "Remove snapshot '{name}' from project '{}'?",
                        project.name
                    ))
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
            SnapshotManager::new()?.delete_snapshot(project.scope(), &name)?;
            vm_success!("Removed snapshot '{name}'");
            Ok(())
        }
        SnapshotSubcommand::Export {
            name,
            env,
            output,
            compression,
            overwrite,
        } => {
            let project = snapshot_project(config_path, profile)?;
            let snapshot = metadata(&project, &name)?;
            verify_environment_filter(&snapshot, env.as_deref())?;
            let provider = snapshot.provider;
            if output.exists() && !overwrite {
                return Err(VmError::validation(
                    format!("Export destination '{}' already exists", output.display()),
                    Some("Choose a new output path"),
                ));
            }
            vm_snapshot::handle_export(
                &provider,
                &name,
                Some(&output),
                compression,
                Some(&project.name),
                Some(&project.config_path),
                overwrite,
            )
            .await?;
            vm_success!("Exported snapshot '{name}' to {}", output.display());
            Ok(())
        }
        SnapshotSubcommand::Import { archive, name } => {
            let config = AppConfig::load(config_path, profile, None)?;
            require_project_config(&config.vm)?;
            let project = project_name(&config.vm);
            vm_snapshot::handle_import(
                &archive,
                Some(&name),
                Some(project),
                config.vm.owning_config_path(),
                false,
            )
            .await?;
            vm_success!("Imported snapshot '{name}'");
            Ok(())
        }
    }
}

fn ensure_snapshot_provider(provider: &str) -> VmResult<()> {
    if matches!(provider, "docker" | "podman")
        || (provider == "tart" && cfg!(any(feature = "tart", target_os = "macos")))
    {
        Ok(())
    } else {
        Err(VmError::validation(
            format!("Snapshots are not supported by provider '{provider}'"),
            None::<String>,
        ))
    }
}

#[cfg(any(feature = "tart", target_os = "macos"))]
fn tart_home(config: &vm_config::config::VmConfig, project: &str) -> VmResult<Option<PathBuf>> {
    vm_provider::tart_project_home(config, project).map_err(Into::into)
}

#[cfg(not(any(feature = "tart", target_os = "macos")))]
fn tart_home(_config: &vm_config::config::VmConfig, _project: &str) -> VmResult<Option<PathBuf>> {
    Err(VmError::validation(
        "Tart provider support is not enabled in this build",
        None::<String>,
    ))
}

fn snapshot_project_dir(
    explicit_config: Option<&std::path::Path>,
    subject: &super::command_context::RuntimeSubject,
) -> VmResult<PathBuf> {
    let config_path = if let Some(path) = explicit_config {
        Some(path.to_path_buf())
    } else {
        subject.provider.instance_config_path(&subject.target)?
    };
    if let Some(path) = config_path {
        return Ok(path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf());
    }
    std::env::current_dir()
        .map_err(|error| VmError::general(error, "Cannot resolve snapshot project directory"))
}

struct ProjectSnapshotScope {
    name: String,
    config_path: PathBuf,
}

impl ProjectSnapshotScope {
    fn scope(&self) -> SnapshotScope<'_> {
        SnapshotScope::OwnedProject {
            name: &self.name,
            config_path: &self.config_path,
        }
    }
}

fn snapshot_project(
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<ProjectSnapshotScope> {
    let config = AppConfig::load(config_path, profile, None)?;
    require_project_config(&config.vm)?;
    Ok(ProjectSnapshotScope {
        name: project_name(&config.vm).to_string(),
        config_path: config
            .vm
            .owning_config_path()
            .ok_or_else(|| {
                VmError::validation(
                    "Snapshot operation requires a project configuration",
                    None::<String>,
                )
            })?
            .to_path_buf(),
    })
}

fn verify_environment_filter(metadata: &SnapshotMetadata, selected: Option<&str>) -> VmResult<()> {
    if selected.is_some_and(|name| metadata.source_environment.as_deref() != Some(name)) {
        return Err(VmError::validation(
            format!(
                "Snapshot '{}' was not captured from environment '{}'",
                metadata.name,
                selected.unwrap_or_default()
            ),
            Some("Select the snapshot's source environment or omit --env"),
        ));
    }
    Ok(())
}

fn metadata(project: &ProjectSnapshotScope, name: &str) -> VmResult<SnapshotMetadata> {
    let path = SnapshotManager::new()?
        .get_snapshot_dir(project.scope(), name)?
        .join("metadata.json");
    if !path.is_file() {
        return Err(VmError::validation(
            format!(
                "Snapshot '{name}' was not found in project '{}'",
                project.name
            ),
            None::<String>,
        ));
    }
    let metadata = SnapshotMetadata::load(&path)?;
    if metadata.project_name != project.name
        || metadata.owner_config_path.as_deref()
            != Some(
                project
                    .config_path
                    .canonicalize()?
                    .to_string_lossy()
                    .as_ref(),
            )
    {
        return Err(VmError::validation(
            format!("Snapshot '{name}' has a different project owner"),
            None::<String>,
        ));
    }
    Ok(metadata)
}

#[cfg(test)]
mod tests {
    use super::snapshot_project;

    #[test]
    fn snapshot_scope_resolves_from_project_without_a_runtime() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("vm.yaml");
        std::fs::write(
            &config,
            "version: '2.0'\nproject:\n  name: example\nprovider: docker\n",
        )
        .unwrap();
        assert_eq!(
            snapshot_project(Some(config), None).unwrap().name,
            "example"
        );
    }
}
