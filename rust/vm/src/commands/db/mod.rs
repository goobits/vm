//! DB subcommand handlers

pub mod backup;
pub(crate) mod route;
pub mod utils;

use crate::cli::{DbBackupSubcommand, DbSubcommand};
use crate::error::VmResult;
use route::DbRoute;
use std::path::PathBuf;
use vm_config::GlobalConfig;
use vm_core::{vm_println, vm_progress, vm_success};

async fn show_credentials(service_name: &str, reveal: bool) -> VmResult<()> {
    if service_name != "postgresql" {
        return Err(crate::error::VmError::validation(
            format!("Service '{service_name}' is not the configured PostgreSQL service"),
            Some("Use `vm db credentials postgresql`"),
        ));
    }
    backup::validate_backup_component(service_name)?;
    let secrets_dir = vm_core::user_paths::secrets_dir()?;
    let secret_file = secrets_dir.join(format!("{}.env", service_name));

    if secret_file.exists() {
        if reveal {
            let password = tokio::fs::read_to_string(secret_file).await?;
            vm_println!("{}", password.trim_end());
        } else {
            vm_println!("Credentials for '{}': available (redacted)", service_name);
        }
    } else {
        vm_println!(
            "No credentials found for service '{}'. Has it been started yet?",
            service_name
        );
    }
    Ok(())
}

pub async fn handle_db(
    command: DbSubcommand,
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
    let global_config = GlobalConfig::load()?;
    let environment = match &command {
        DbSubcommand::Backups { command } => match command {
            DbBackupSubcommand::Create { env, .. }
            | DbBackupSubcommand::Restore { env, .. }
            | DbBackupSubcommand::List { env, .. }
            | DbBackupSubcommand::Remove { env, .. } => env.clone(),
        },
        DbSubcommand::List { env }
        | DbSubcommand::Status { env, .. }
        | DbSubcommand::Export { env, .. }
        | DbSubcommand::Import { env, .. }
        | DbSubcommand::Reset { env, .. }
        | DbSubcommand::Credentials { env, .. } => env.clone(),
    };
    let route = DbRoute::load(config_path, profile, environment)?;

    match command {
        DbSubcommand::Backups {
            command:
                DbBackupSubcommand::Create {
                    name,
                    database,
                    all,
                    ..
                },
        } => {
            if all {
                vm_progress!("Backing up configured database '{}'...", route.database);
                backup::backup_db(
                    &route,
                    &route.database,
                    Some(&name),
                    global_config.backups.keep_count,
                )
                .await?;
                vm_success!("Backed up database '{}'", route.database);
            } else if let Some(db) = database {
                route.require_database(&db)?;
                backup::backup_db(&route, &db, Some(&name), global_config.backups.keep_count)
                    .await?;
            } else {
                return Err(crate::error::VmError::validation(
                    "Missing database name",
                    Some(
                        "Provide a database name or use --all to backup all databases".to_string(),
                    ),
                ));
            }
        }
        DbSubcommand::Backups {
            command:
                DbBackupSubcommand::Restore {
                    backup,
                    database,
                    yes,
                    ..
                },
        } => {
            route.require_database(&database)?;
            backup::restore_db(&route, &backup, &database, yes).await?;
        }
        DbSubcommand::Backups {
            command: DbBackupSubcommand::List { database, .. },
        } => {
            if let Some(name) = &database {
                route.require_database(name)?;
            }
            for item in backup::list_backups(&route, Some(&route.database))? {
                vm_println!("{item}");
            }
        }
        DbSubcommand::Backups {
            command: DbBackupSubcommand::Remove { backup, yes, .. },
        } => {
            backup::remove_backup(&route, &backup, yes)?;
        }
        DbSubcommand::List { .. } => {
            let query = format!("SELECT datname, pg_size_pretty(pg_database_size(datname)) FROM pg_database WHERE datname = {};", backup::quote_pg_literal(&route.database));
            let result = utils::execute_psql_command(&route, &query).await?;

            vm_println!("Database for environment '{}':", route.environment);
            for line in result.lines() {
                let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
                if parts.len() == 2 && !parts[0].is_empty() {
                    let db_name = parts[0];
                    let db_size = parts[1];
                    let backup_count = backup::count_backups(&route, db_name).await.unwrap_or(0);

                    if backup_count > 0 {
                        vm_println!(
                            "  - {:<30} {} ({} backup{})",
                            db_name,
                            db_size,
                            backup_count,
                            if backup_count == 1 { "" } else { "s" }
                        );
                    } else {
                        vm_println!("  - {:<30} {} (no backups)", db_name, db_size);
                    }
                }
            }

            if let Ok(backup_path) = backup::get_backup_path(&route) {
                vm_println!("\n💾 Backups stored in: {}", backup_path);
            }
        }
        DbSubcommand::Status { name, .. } => {
            route.require_database(&name)?;
            let query = format!("SELECT datname, pg_size_pretty(pg_database_size(datname)) FROM pg_database WHERE datname = {};", backup::quote_pg_literal(&name));
            let result = utils::execute_psql_command(&route, &query).await?;
            if result.trim().is_empty() {
                return Err(crate::error::VmError::validation(
                    format!("Database '{name}' not found"),
                    None::<String>,
                ));
            }
            vm_println!("{}", result.trim());
            vm_println!("Backups: {}", backup::count_backups(&route, &name).await?);
        }
        DbSubcommand::Export {
            name,
            output,
            overwrite,
            ..
        } => {
            route.require_database(&name)?;
            backup::export_db(&route, &name, &output, overwrite).await?;
        }
        DbSubcommand::Import {
            name, file, yes, ..
        } => {
            route.require_database(&name)?;
            backup::import_db(&route, &name, &file, yes).await?;
        }
        DbSubcommand::Reset { name, yes, .. } => {
            route.require_database(&name)?;
            backup::reset_db(&route, &name, yes).await?;
        }
        DbSubcommand::Credentials {
            service, reveal, ..
        } => {
            show_credentials(&service, reveal).await?;
        }
    }
    Ok(())
}
