use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use chrono::Local;
use vm_core::error::{Result, VmError};
use vm_core::user_paths;
use vm_core::{vm_info, vm_success, vm_warning};

/// Represents a single configuration file to be migrated.
struct MigrationPath {
    old_path_fn: fn() -> Result<PathBuf>,
    new_path_fn: fn() -> Result<PathBuf>,
}

/// Defines all the configuration files that are subject to migration.
const MIGRATION_PATHS: &[MigrationPath] = &[
    MigrationPath {
        old_path_fn: || Ok(user_paths::user_config_dir()?.join("global.yaml")),
        new_path_fn: || Ok(user_paths::vm_state_dir()?.join("config.yaml")),
    },
    MigrationPath {
        old_path_fn: || Ok(user_paths::vm_state_dir()?.join("port-registry.json")),
        new_path_fn: || Ok(user_paths::vm_state_dir()?.join("ports.json")),
    },
    MigrationPath {
        old_path_fn: || Ok(user_paths::vm_state_dir()?.join("service_state.json")),
        new_path_fn: || Ok(user_paths::vm_state_dir()?.join("services.json")),
    },
    MigrationPath {
        old_path_fn: || Ok(user_paths::vm_state_dir()?.join("temp-vm.state")),
        new_path_fn: || Ok(user_paths::vm_state_dir()?.join("temp-vms.json")),
    },
];

/// A file that has been identified as needing migration.
struct PendingMigration {
    old_path: PathBuf,
    new_path: PathBuf,
}

/// Migrate all legacy configuration files to their new locations.
pub fn migrate_config_files() -> Result<()> {
    vm_info!("Checking for old configuration files...");

    let pending_migrations = find_pending_migrations()?;

    if pending_migrations.is_empty() {
        vm_success!("All configuration files are already up to date. No migration needed.");
        return Ok(());
    }

    display_pending_migrations(&pending_migrations);

    if !confirm_migration()? {
        vm_warning!("Migration cancelled by user.");
        return Ok(());
    }

    execute_migration(pending_migrations)?;

    vm_success!("Migration complete!");
    Ok(())
}

/// Finds all configuration files that exist at the old path but not the new one.
fn find_pending_migrations() -> Result<Vec<PendingMigration>> {
    let mut pending = Vec::new();
    for path_info in MIGRATION_PATHS {
        let old_path = (path_info.old_path_fn)()?;
        let new_path = (path_info.new_path_fn)()?;

        if old_path.exists() && !new_path.exists() {
            pending.push(PendingMigration { old_path, new_path });
        }
    }
    Ok(pending)
}

/// Displays the files that will be migrated to the user.
fn display_pending_migrations(migrations: &[PendingMigration]) {
    println!("\nFound old configuration files to migrate:");
    for migration in migrations {
        println!(
            "  • {} → {}",
            migration.old_path.display(),
            migration.new_path.display()
        );
    }
}

/// Prompts the user for confirmation to proceed with the migration.
fn confirm_migration() -> Result<bool> {
    print!("\nMigrate these files? [Y/n] ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let choice = input.trim().to_lowercase();
    Ok(choice.is_empty() || choice == "y")
}

/// Performs the file migration, including backups.
fn execute_migration(migrations: Vec<PendingMigration>) -> Result<()> {
    let backup_dir = create_backup_dir()?;
    println!();

    for migration in migrations {
        let file_name = migration.old_path.file_name().ok_or_else(|| {
            VmError::Migration(format!(
                "Could not get file name from path: {}",
                migration.old_path.display()
            ))
        })?;
        let backup_path = backup_dir.join(file_name);

        // Create backup
        fs::copy(&migration.old_path, &backup_path).map_err(|e| {
            VmError::Migration(format!(
                "Failed to back up {}: {}",
                migration.old_path.display(),
                e
            ))
        })?;

        // Move file
        fs::rename(&migration.old_path, &migration.new_path).map_err(|e| {
            VmError::Migration(format!(
                "Failed to move {} to {}: {}",
                migration.old_path.display(),
                migration.new_path.display(),
                e
            ))
        })?;

        vm_success!(
            "Migrated {} → {}",
            migration.old_path.display(),
            migration.new_path.display()
        );
    }

    vm_info!("\nOld files backed up to {}", backup_dir.display());
    Ok(())
}

/// Creates a timestamped backup directory inside ~/.vm/backups.
fn create_backup_dir() -> Result<PathBuf> {
    let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S");
    let backup_dir = user_paths::vm_state_dir()?
        .join("backups")
        .join(timestamp.to_string());

    fs::create_dir_all(&backup_dir).map_err(|e| {
        VmError::Migration(format!(
            "Failed to create backup directory at {}: {}",
            backup_dir.display(),
            e
        ))
    })?;

    Ok(backup_dir)
}
