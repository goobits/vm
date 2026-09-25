//! DB subcommand handlers

pub mod backup;
pub(crate) mod route;
pub mod utils;

use crate::cli::{DbBackupSubcommand, DbSubcommand};
use crate::error::VmResult;
use route::DbRoute;
use std::path::PathBuf;
use vm_config::GlobalConfig;
use vm_core::{vm_println, vm_progress, vm_success, vm_warning};

async fn show_credentials(route: &DbRoute, service_name: &str, reveal: bool) -> VmResult<()> {
    if service_name != "postgresql" {
        return Err(crate::error::VmError::validation(
            format!("Service '{service_name}' is not the configured PostgreSQL service"),
            Some("Use `vm db credentials postgresql`"),
        ));
    }
    let secrets_dir = vm_core::user_paths::secrets_dir()?;
    let secret_file = secrets_dir.join(format!("{}.env", service_name));

    vm_println!("Service: {} ({})", service_name, route.environment);
    vm_println!("Container: {}", route.container);
    vm_println!("User: {}", route.user);
    vm_println!("Default database: {}", route.database);
    if reveal {
        let password = if let Some(configured) = &route.configured_password {
            configured.as_str().to_string()
        } else {
            tokio::fs::read_to_string(&secret_file)
                .await
                .map_err(|error| {
                    crate::error::VmError::general(
                        error,
                        "Selected PostgreSQL credential is unavailable",
                    )
                })?
                .trim()
                .to_string()
        };
        vm_println!("Password: {}", password);
    } else if route.configured_password.is_some() || secret_file.exists() {
        vm_println!("Password: available (redacted)");
    } else {
        vm_println!("Password: unavailable");
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
                let databases = utils::list_databases(&route).await?;
                if databases.is_empty() {
                    vm_println!("No user databases found in '{}'.", route.container);
                    return Ok(());
                }
                vm_progress!(
                    "Backing up {} database(s) in '{}'...",
                    databases.len(),
                    route.container
                );
                let mut succeeded = 0;
                let mut failed = 0;
                for db in databases {
                    match backup::backup_db(
                        &route,
                        &db,
                        Some(&name),
                        global_config.backups.keep_count,
                    )
                    .await
                    {
                        Ok(()) => {
                            vm_success!("Backup member '{db}': succeeded");
                            succeeded += 1;
                        }
                        Err(error) => {
                            vm_warning!("Backup member '{db}': failed: {error}");
                            failed += 1;
                        }
                    }
                }
                vm_println!("Backup members: {succeeded} succeeded, {failed} failed; databases are backed up independently");
                if failed > 0 {
                    return Err(crate::error::VmError::validation(
                        format!("Database backup had {failed} failed member(s)"),
                        Some("Inspect the member errors above, then retry the backup"),
                    ));
                }
            } else if let Some(db) = database {
                DbRoute::validate_database_name(&db)?;
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
            DbRoute::validate_database_name(&database)?;
            backup::restore_db(&route, &backup, &database, yes).await?;
        }
        DbSubcommand::Backups {
            command: DbBackupSubcommand::List { database, .. },
        } => {
            if let Some(name) = &database {
                DbRoute::validate_database_name(name)?;
            }
            for item in backup::list_backups(&route, database.as_deref())? {
                vm_println!("{item}");
            }
        }
        DbSubcommand::Backups {
            command: DbBackupSubcommand::Remove { backup, yes, .. },
        } => {
            backup::remove_backup(&route, &backup, yes)?;
        }
        DbSubcommand::List { .. } => {
            let query = "SELECT datname, pg_size_pretty(pg_database_size(datname)) FROM pg_database WHERE datistemplate = false AND datname <> 'postgres' ORDER BY datname;";
            let result = utils::execute_psql_command(&route, query).await?;

            vm_println!(
                "Databases in '{}' for environment '{}':",
                route.container,
                route.environment
            );
            for line in result.lines() {
                let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
                if parts.len() == 2 && !parts[0].is_empty() {
                    let db_name = parts[0];
                    let db_size = parts[1];
                    let backup_count = backup::count_backups(&route, db_name).await?;

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
            DbRoute::validate_database_name(&name)?;
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
            DbRoute::validate_database_name(&name)?;
            backup::export_db(&route, &name, &output, overwrite).await?;
        }
        DbSubcommand::Import {
            name, file, yes, ..
        } => {
            DbRoute::validate_database_name(&name)?;
            backup::import_db(&route, &name, &file, yes).await?;
        }
        DbSubcommand::Reset { name, yes, .. } => {
            DbRoute::validate_database_name(&name)?;
            backup::reset_db(&route, &name, yes).await?;
        }
        DbSubcommand::Credentials {
            service, reveal, ..
        } => {
            show_credentials(&route, &service, reveal).await?;
        }
    }
    Ok(())
}
