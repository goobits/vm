//! VM snapshot management - thin wrapper around vm-snapshot crate

use crate::cli::SnapshotSubcommand;
use crate::error::VmResult;
use std::path::PathBuf;
use vm_config::AppConfig;

// Re-export modules and types for internal use
pub use vm_snapshot::manager;
pub use vm_snapshot::metadata;

pub async fn handle_snapshot(
    command: SnapshotSubcommand,
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
    let app_config = AppConfig::load(config_path, profile, None)?;
    let executable = app_config.vm.provider.as_deref().unwrap_or("docker");
    let current_project = app_config
        .vm
        .project
        .as_ref()
        .and_then(|project| project.name.clone());

    match command {
        SnapshotSubcommand::Create {
            name,
            description,
            quiesce,
            project,
            from_dockerfile,
            build_context,
            build_arg,
            force,
        } => {
            vm_snapshot::create::handle_create(
                &app_config,
                executable,
                &name,
                description.as_deref(),
                quiesce,
                project.as_deref(),
                from_dockerfile.as_deref(),
                build_context.as_deref(),
                &build_arg,
                force,
            )
            .await?;
        }
        SnapshotSubcommand::List { project, r#type } => {
            let project = project.or_else(|| {
                if r#type.as_deref() == Some("base") {
                    None
                } else {
                    current_project.clone()
                }
            });
            vm_snapshot::manager::handle_list(project.as_deref(), r#type.as_deref(), false).await?;
        }
        SnapshotSubcommand::Restore {
            name,
            project,
            force,
        } => {
            let project = project.or_else(|| current_project.clone());
            vm_snapshot::restore::handle_restore(
                &app_config,
                executable,
                &name,
                project.as_deref(),
                force,
            )
            .await?;
        }
        SnapshotSubcommand::Delete {
            name,
            project,
            force,
        } => {
            let project = project.or_else(|| current_project.clone());
            vm_snapshot::manager::handle_delete(&name, project.as_deref(), force).await?;
        }
        SnapshotSubcommand::Export {
            name,
            output,
            compress,
            project,
        } => {
            let project = project.or_else(|| current_project.clone());
            vm_snapshot::export::handle_export(
                executable,
                &name,
                output.as_deref(),
                compress,
                project.as_deref(),
            )
            .await?;
        }
        SnapshotSubcommand::Import {
            file,
            name,
            verify,
            force,
        } => {
            vm_snapshot::import::handle_import(executable, &file, name.as_deref(), verify, force)
                .await?;
        }
    }

    Ok(())
}
