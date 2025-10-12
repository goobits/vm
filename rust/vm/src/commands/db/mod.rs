//! DB subcommand handlers

pub mod backup;
pub mod utils;

use crate::cli::DbSubcommand;
use vm_config::GlobalConfig;
use vm_core::vm_println;
use crate::error::VmResult;

pub async fn handle_db(command: DbSubcommand) -> VmResult<()> {
    let global_config = GlobalConfig::load()?;

    match command {
        DbSubcommand::Backup { db_name, name } => {
            backup::backup_db(
                &db_name,
                name.as_deref(),
                global_config.services.postgresql.backup_retention,
            )
            .await?;
        }
        DbSubcommand::Restore { name, db_name } => {
            backup::restore_db(&name, &db_name).await?;
        }
        DbSubcommand::List => {
            let result =
                utils::execute_psql_command("SELECT datname FROM pg_database WHERE datistemplate = false;")
                    .await?;
            vm_println!("Databases:");
            for line in result.lines() {
                let db_name = line.trim();
                if !db_name.is_empty() {
                    vm_println!("  - {}", db_name);
                }
            }
        }
        DbSubcommand::Export { name, file } => {
            backup::export_db(&name, &file).await?;
        }
        DbSubcommand::Import { file, db_name } => {
            backup::import_db(&db_name, &file).await?;
        }
        DbSubcommand::Size => {
            let result = utils::execute_psql_command(
                "SELECT datname, pg_size_pretty(pg_database_size(datname)) FROM pg_database WHERE datistemplate = false;",
            )
            .await?;
            vm_println!("Database Sizes:");
            for line in result.lines() {
                let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
                if parts.len() == 2 && !parts[0].is_empty() {
                    vm_println!("  - {:<30} {}", parts[0], parts[1]);
                }
            }
        }
        DbSubcommand::Reset { name, force } => {
            backup::reset_db(&name, force).await?;
        }
    }
    Ok(())
}
