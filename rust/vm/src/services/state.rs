use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::error::VmError;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct ServiceState {
    pub reference_count: u32,
    pub is_running: bool,
    pub port: u16,
    pub pid: Option<u32>,
    pub registered_vms: Vec<String>,
}

#[derive(Clone)]
pub(super) struct ServiceStateStore {
    values: Arc<Mutex<HashMap<String, ServiceState>>>,
    path: PathBuf,
}

impl ServiceStateStore {
    pub(super) fn new(path: PathBuf) -> Self {
        Self {
            values: Arc::new(Mutex::new(HashMap::new())),
            path,
        }
    }

    pub(super) fn get(&self, service: &str) -> Option<ServiceState> {
        self.values
            .lock()
            .ok()
            .and_then(|values| values.get(service).cloned())
    }

    pub(super) fn update<T>(
        &self,
        update: impl FnOnce(&mut HashMap<String, ServiceState>) -> T,
    ) -> Result<T> {
        let mut values = self.lock()?;
        Ok(update(&mut values))
    }

    pub(super) fn save(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&*self.lock()?)
            .context("Failed to serialize service state")?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).context("Failed to create service state directory")?;
        }
        vm_core::file_system::atomic_write(&self.path, json.as_bytes())
            .context("Failed to write service state file")?;
        debug!(path = %self.path.display(), "Service state saved");
        Ok(())
    }

    pub(super) fn load(&self, is_known: impl Fn(&str) -> bool) -> Result<()> {
        if !self.path.exists() {
            debug!(path = %self.path.display(), "No existing service state file found");
            return Ok(());
        }
        let content =
            std::fs::read_to_string(&self.path).context("Failed to read service state file")?;
        let mut loaded: HashMap<String, ServiceState> =
            serde_json::from_str(&content).context("Failed to parse service state file")?;
        loaded.retain(|name, _| is_known(name));
        for state in loaded.values_mut() {
            state.is_running = false;
            state.pid = None;
        }
        *self.lock()? = loaded;
        info!(path = %self.path.display(), "Service state loaded");
        Ok(())
    }

    fn lock(&self) -> Result<MutexGuard<'_, HashMap<String, ServiceState>>> {
        self.values.lock().map_err(|error| {
            VmError::general(
                std::io::Error::new(std::io::ErrorKind::Other, error.to_string()),
                "Service state mutex was poisoned",
            )
            .into()
        })
    }
}
