use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info, warn};
use vm_core::error::Result;

pub trait Migration {
    fn id(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn run(&self) -> Result<()>;
}

#[derive(Serialize, Deserialize, Default)]
struct MigrationState {
    completed: HashSet<String>,
    version_info: std::collections::HashMap<String, String>,
}

impl MigrationState {
    fn mark_completed(&mut self, migration: &dyn Migration) {
        self.completed.insert(migration.id().to_string());
        self.version_info
            .insert(migration.id().to_string(), migration.version().to_string());
    }

    fn is_completed(&self, migration_id: &str) -> bool {
        self.completed.contains(migration_id)
    }
}

pub fn run_pending_migrations() -> Result<()> {
    let migrations: Vec<Box<dyn Migration>> = vec![Box::new(m001_unify_paths::UnifyPaths)];

    let mut state = load_migration_state()?;

    for migration in migrations {
        if !state.is_completed(migration.id()) {
            info!(
                "Running migration {}: {} ({})",
                migration.id(),
                migration.description(),
                migration.version()
            );

            migration.run().map_err(|e| {
                warn!("Migration {} failed: {}", migration.id(), e);
                e
            })?;

            state.mark_completed(migration.as_ref());
            save_migration_state(&state)?;
            info!("Migration {} completed successfully", migration.id());
        } else {
            debug!("Migration {} already completed", migration.id());
        }
    }

    Ok(())
}

fn get_migration_state_path() -> Result<PathBuf> {
    Ok(crate::user_paths::vm_state_dir()?.join("migrations.json"))
}

fn load_migration_state() -> Result<MigrationState> {
    let state_path = get_migration_state_path()?;

    if !state_path.exists() {
        return Ok(MigrationState::default());
    }

    let contents = fs::read_to_string(&state_path)?;
    let state: MigrationState =
        serde_json::from_str(&contents).unwrap_or_else(|_| MigrationState::default());

    Ok(state)
}

fn save_migration_state(state: &MigrationState) -> Result<()> {
    let state_path = get_migration_state_path()?;

    if let Some(parent) = state_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let contents = serde_json::to_string_pretty(state)?;
    fs::write(state_path, contents)?;

    Ok(())
}

mod m001_unify_paths;
