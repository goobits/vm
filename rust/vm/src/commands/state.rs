use serde::Serialize;
use std::path::PathBuf;

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
        SnapshotSubcommand::List { env, json } => {
            let project = snapshot_project(config_path, profile)?;
            let manager = SnapshotManager::new()?;
            let snapshots = manager.list_snapshots_in_scope(project.scope())?;
            let selected = snapshots
                .iter()
                .filter(|snapshot| {
                    env.as_deref().map_or(true, |selected| {
                        snapshot.source_environment.as_deref() == Some(selected)
                    })
                })
                .collect::<Vec<_>>();
            if json {
                let views = selected
                    .iter()
                    .map(|snapshot| snapshot_view(&manager, &project, snapshot))
                    .collect::<VmResult<Vec<_>>>()?;
                return crate::presentation::success("snapshots list", views);
            }
            for snapshot in selected {
                vm_println!(
                    "{}\t{}\t{} bytes",
                    snapshot.name,
                    snapshot.created_at,
                    snapshot.total_size_bytes
                );
            }
            Ok(())
        }
        SnapshotSubcommand::Show { name, env, json } => {
            let project = snapshot_project(config_path, profile)?;
            let metadata = metadata(&project, &name)?;
            verify_environment_filter(&metadata, env.as_deref())?;
            if json {
                let manager = SnapshotManager::new()?;
                return crate::presentation::success(
                    "snapshots show",
                    snapshot_view(&manager, &project, &metadata)?,
                );
            }
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
            json,
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
            report_change(
                "snapshots create",
                SnapshotScope::OwnedProject {
                    name: &project,
                    config_path: config.vm.owning_config_path().ok_or_else(|| {
                        VmError::validation(
                            "Snapshot creation requires a project configuration",
                            None::<String>,
                        )
                    })?,
                },
                &name,
                Some(&target),
                json,
                &format!("Created snapshot '{name}'"),
            )
        }
        SnapshotSubcommand::Restore {
            name,
            env,
            yes,
            json,
        } => {
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
            if !crate::confirmation::destructive(
                &format!("Restore snapshot '{name}' into '{target}'?"),
                yes,
            )? {
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
                #[cfg(feature = "tart")]
                vm_provider::validate_tart_restore_target(&target, &config.vm)?;
                vm_snapshot::handle_tart_restore(&name, &project, &target, home.as_deref(), &owner)
                    .await?;
                #[cfg(feature = "tart")]
                vm_provider::refresh_tart_runtime_identity(&target, home.as_deref(), &config.vm)?;
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
            report_change(
                "snapshots restore",
                SnapshotScope::OwnedProject {
                    name: &project,
                    config_path: &owner,
                },
                &name,
                Some(&target),
                json,
                &format!("Restored snapshot '{name}' into '{target}'"),
            )
        }
        SnapshotSubcommand::Remove {
            name,
            env,
            yes,
            json,
        } => {
            let project = snapshot_project(config_path, profile)?;
            let snapshot = metadata(&project, &name)?;
            verify_environment_filter(&snapshot, env.as_deref())?;
            if !crate::confirmation::destructive(
                &format!("Remove snapshot '{name}' from project '{}'?", project.name),
                yes,
            )? {
                return Err(VmError::validation(
                    "Snapshot removal cancelled",
                    None::<String>,
                ));
            }
            SnapshotManager::new()?.delete_snapshot(project.scope(), &name)?;
            report_change(
                "snapshots remove",
                project.scope(),
                &name,
                snapshot.source_environment.as_deref(),
                json,
                &format!("Removed snapshot '{name}'"),
            )
        }
        SnapshotSubcommand::Export {
            name,
            json,
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
                match compression {
                    crate::cli::SnapshotCompression::Gzip => vm_snapshot::ArchiveCompression::Gzip,
                    crate::cli::SnapshotCompression::None => vm_snapshot::ArchiveCompression::None,
                },
                Some(&project.name),
                Some(&project.config_path),
                overwrite,
            )
            .await?;
            report_change(
                "snapshots export",
                project.scope(),
                &name,
                snapshot.source_environment.as_deref(),
                json,
                &format!("Exported snapshot '{name}' to {}", output.display()),
            )
        }
        SnapshotSubcommand::Import {
            archive,
            name,
            json,
        } => {
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
            report_change(
                "snapshots import",
                SnapshotScope::OwnedProject {
                    name: project,
                    config_path: config.vm.owning_config_path().ok_or_else(|| {
                        VmError::validation(
                            "Snapshot import requires a project configuration",
                            None::<String>,
                        )
                    })?,
                },
                &name,
                None,
                json,
                &format!("Imported snapshot '{name}'"),
            )
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

#[derive(Serialize)]
struct SnapshotView<'a> {
    id: String,
    name: &'a str,
    project: &'a str,
    environment: Option<&'a str>,
    provider: &'a str,
    architecture: &'a str,
    consistency: &'a str,
    created_at: &'a chrono::DateTime<chrono::Utc>,
    size_bytes: u64,
    service_count: usize,
    volume_count: usize,
    native_image_digest: Option<&'a str>,
}

#[derive(Serialize)]
struct SnapshotChange<'a> {
    id: String,
    environment: Option<&'a str>,
}

fn report_change(
    command: &'static str,
    scope: SnapshotScope<'_>,
    name: &str,
    environment: Option<&str>,
    json: bool,
    message: &str,
) -> VmResult<()> {
    if json {
        crate::presentation::success(
            command,
            SnapshotChange {
                id: SnapshotManager::new()?.snapshot_id(scope, name)?,
                environment,
            },
        )
    } else {
        vm_success!("{message}");
        Ok(())
    }
}

fn snapshot_view<'a>(
    manager: &SnapshotManager,
    project: &ProjectSnapshotScope,
    metadata: &'a SnapshotMetadata,
) -> VmResult<SnapshotView<'a>> {
    Ok(SnapshotView {
        id: manager.snapshot_id(project.scope(), &metadata.name)?,
        name: &metadata.name,
        project: &metadata.project_name,
        environment: metadata.source_environment.as_deref(),
        provider: &metadata.provider,
        architecture: &metadata.architecture,
        consistency: &metadata.consistency,
        created_at: &metadata.created_at,
        size_bytes: metadata.total_size_bytes,
        service_count: metadata.services.len(),
        volume_count: metadata.volumes.len(),
        native_image_digest: metadata.native_image_digest.as_deref(),
    })
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
        )
        .with_target(metadata.name.clone()));
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
        )
        .with_target(name));
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
