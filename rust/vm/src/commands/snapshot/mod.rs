//! VM snapshot management
pub mod create;
pub mod export;
pub mod import;
pub mod manager;
pub mod metadata;
pub mod restore;

use crate::cli::SnapshotSubcommand;
use crate::error::VmResult;
use std::path::PathBuf;
use vm_config::AppConfig;

pub async fn handle_snapshot(
    command: SnapshotSubcommand,
    config_path: Option<PathBuf>,
) -> VmResult<()> {
    let app_config = AppConfig::load(config_path)?;

    match command {
        SnapshotSubcommand::Create {
            name,
            description,
            quiesce,
            project,
            from_dockerfile,
            build_arg,
        } => {
            create::handle_create(
                &app_config,
                &name,
                description.as_deref(),
                quiesce,
                project.as_deref(),
                from_dockerfile.as_deref(),
                &build_arg,
            )
            .await
        }
        SnapshotSubcommand::List { project } => manager::handle_list(project.as_deref()).await,
        SnapshotSubcommand::Restore {
            name,
            project,
            force,
        } => restore::handle_restore(&app_config, &name, project.as_deref(), force).await,
        SnapshotSubcommand::Delete {
            name,
            project,
            force,
        } => manager::handle_delete(&name, project.as_deref(), force).await,
        SnapshotSubcommand::Export {
            name,
            output,
            compress,
            project,
        } => export::handle_export(&name, output.as_deref(), compress, project.as_deref()).await,
        SnapshotSubcommand::Import {
            file,
            name,
            verify,
            force,
        } => import::handle_import(&file, name.as_deref(), verify, force).await,
    }
}
