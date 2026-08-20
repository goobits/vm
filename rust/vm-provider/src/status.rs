use std::path::PathBuf;

/// Lightweight lifecycle state returned without collecting metrics or services.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceState {
    Running,
    Stopped,
    Starting,
    Paused,
    Suspended,
    Unknown(String),
}

impl InstanceState {
    pub fn from_runtime_status(status: &str) -> Self {
        match status.trim().to_ascii_lowercase().as_str() {
            "running" => Self::Running,
            "created" | "exited" | "dead" | "stopped" => Self::Stopped,
            "restarting" | "starting" | "removing" => Self::Starting,
            "paused" => Self::Paused,
            "suspended" => Self::Suspended,
            other => Self::Unknown(other.to_string()),
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running)
    }
}

impl Default for InstanceState {
    fn default() -> Self {
        Self::Unknown("unknown".to_string())
    }
}

impl std::fmt::Display for InstanceState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Running => formatter.write_str("running"),
            Self::Stopped => formatter.write_str("stopped"),
            Self::Starting => formatter.write_str("starting"),
            Self::Paused => formatter.write_str("paused"),
            Self::Suspended => formatter.write_str("suspended"),
            Self::Unknown(state) => formatter.write_str(state),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResourceUsage {
    pub cpu_percent: Option<f64>,
    pub memory_used_mb: Option<u64>,
    pub memory_limit_mb: Option<u64>,
    pub disk_used_gb: Option<f64>,
    pub disk_total_gb: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct MountUsage {
    pub target: String,
    pub storage_type: String,
    pub name: Option<String>,
    pub used_bytes: Option<u64>,
    pub capacity_bytes: Option<u64>,
    pub options: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeDiagnostics {
    pub generated_config: Option<PathBuf>,
    pub generated_config_exists: bool,
    pub writable_layer_bytes: Option<u64>,
    pub root_filesystem_bytes: Option<u64>,
    pub memory_peak_bytes: Option<u64>,
    pub pids_current: Option<u64>,
    pub pids_peak: Option<u64>,
    pub pids_limit: Option<u64>,
    pub mounts: Vec<MountUsage>,
    pub logging_driver: Option<String>,
    pub logging_options: Vec<(String, String)>,
    pub restart_policy: Option<String>,
    pub stop_timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ServiceStatus {
    pub name: String,
    pub is_running: bool,
    pub port: Option<u16>,
    pub host_port: Option<u16>,
    pub metrics: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct VmStatusReport {
    pub name: String,
    pub provider: String,
    pub container_id: Option<String>,
    pub state: InstanceState,
    pub is_running: bool,
    pub uptime: Option<String>,
    pub resources: ResourceUsage,
    pub services: Vec<ServiceStatus>,
    pub runtime: Option<RuntimeDiagnostics>,
}
