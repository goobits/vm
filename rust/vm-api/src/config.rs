use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,

    #[serde(default = "default_db_path")]
    pub db_path: PathBuf,

    #[serde(default = "default_janitor_interval")]
    pub janitor_interval_secs: u64,

    #[serde(default = "default_provisioner_interval")]
    pub provisioner_interval_secs: u64,
}

fn default_bind_addr() -> String {
    std::env::var("VM_API_BIND").unwrap_or_else(|_| "0.0.0.0:3121".to_string())
}

fn default_db_path() -> PathBuf {
    if let Ok(path) = std::env::var("VM_API_DB_PATH") {
        return PathBuf::from(path);
    }

    if cfg!(windows) {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(appdata).join("vm").join("api").join("vm.db")
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".vm").join("api").join("vm.db")
    }
}

fn default_janitor_interval() -> u64 {
    std::env::var("VM_API_JANITOR_INTERVAL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300) // 5 minutes
}

fn default_provisioner_interval() -> u64 {
    std::env::var("VM_API_PROVISIONER_INTERVAL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10) // 10 seconds
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind_addr: default_bind_addr(),
            db_path: default_db_path(),
            janitor_interval_secs: default_janitor_interval(),
            provisioner_interval_secs: default_provisioner_interval(),
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        Self::default()
    }
}
