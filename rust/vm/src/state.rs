use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use vm_core::error::{Result, VmError};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct VmState {
    pub active_ssh_sessions: u32,
}

impl VmState {
    pub fn load(project_name: &str) -> Result<Self> {
        let state_path = Self::get_state_path(project_name)?;
        if !state_path.exists() {
            return Ok(VmState::default());
        }
        let content = fs::read_to_string(state_path)?;
        serde_json::from_str(&content).map_err(Into::into)
    }

    pub fn save(&self, project_name: &str) -> Result<()> {
        let state_path = Self::get_state_path(project_name)?;
        let state_dir = state_path.parent().ok_or_else(|| {
            VmError::Internal("Could not get parent directory for state file".to_string())
        })?;
        fs::create_dir_all(state_dir)?;
        let content = serde_json::to_string_pretty(self)?;
        fs::write(state_path, content).map_err(Into::into)
    }

    fn get_state_path(project_name: &str) -> Result<PathBuf> {
        let home_dir = dirs::home_dir()
            .ok_or_else(|| VmError::Internal("Could not get home directory".to_string()))?;
        Ok(home_dir.join(format!(".vm/state/{}.json", project_name)))
    }

    pub fn increment_ssh_sessions(&mut self) {
        self.active_ssh_sessions += 1;
    }

    pub fn decrement_ssh_sessions(&mut self) {
        if self.active_ssh_sessions > 0 {
            self.active_ssh_sessions -= 1;
        }
    }
}

pub fn count_active_ssh_sessions(project_name: &str) -> Result<u32> {
    let state = VmState::load(project_name)?;
    Ok(state.active_ssh_sessions)
}
