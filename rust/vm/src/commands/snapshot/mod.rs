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
    let app_config = AppConfig::load(config_path, profile)?;

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
            vm_snapshot::manager::handle_list(project.as_deref(), r#type.as_deref()).await?;
        }
        SnapshotSubcommand::Restore {
            name,
            project,
            force,
        } => {
            vm_snapshot::restore::handle_restore(&app_config, &name, project.as_deref(), force)
                .await?;
        }
        SnapshotSubcommand::Delete {
            name,
            project,
            force,
        } => {
            vm_snapshot::manager::handle_delete(&name, project.as_deref(), force).await?;
        }
        SnapshotSubcommand::Export {
            name,
            output,
            compress,
            project,
        } => {
            vm_snapshot::export::handle_export(
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
            vm_snapshot::import::handle_import(&file, name.as_deref(), verify, force).await?;
        }
    }

    Ok(())
}
