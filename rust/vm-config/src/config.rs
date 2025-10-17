// Standard library imports
use std::fs;
use std::path::{Path, PathBuf};

// External crate imports
use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_yaml_ng as serde_yaml;
use vm_core::error::Result;

// Internal crate imports
use crate::detector::git::GitConfig;
use crate::ports::PortMapping;

// Helper function to deserialize version field that accepts both strings and numbers
fn deserialize_option_string_or_number<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::{Error, Visitor};
    use std::fmt;

    struct StringOrNumberVisitor;

    impl<'de> Visitor<'de> for StringOrNumberVisitor {
        type Value = Option<String>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string, number, or null")
        }

        fn visit_none<E>(self) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_any(InnerVisitor)
        }

        fn visit_unit<E>(self) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(None)
        }
    }

    struct InnerVisitor;

    impl<'de> Visitor<'de> for InnerVisitor {
        type Value = Option<String>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string or number")
        }

        fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(Some(value.to_string()))
        }

        fn visit_i64<E>(self, value: i64) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(Some(value.to_string()))
        }

        fn visit_u64<E>(self, value: u64) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(Some(value.to_string()))
        }

        fn visit_f64<E>(self, value: f64) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(Some(value.to_string()))
        }
    }

    deserializer.deserialize_option(StringOrNumberVisitor)
}

/// Main VM configuration structure.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VmConfig {
    // 1. Metadata & Schema
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    // 2. Provider & Environment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tart: Option<TartConfig>,

    // 3. Project Identity
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectConfig>,

    // 4. VM Resources
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vm: Option<VmSettings>,

    // 5. Runtime Versions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub versions: Option<VersionsConfig>,

    // 6. Networking
    #[serde(default)]
    pub ports: PortsConfig,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub networking: Option<NetworkingConfig>,

    // 7. Services & Infrastructure
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub services: IndexMap<String, ServiceConfig>,

    // 8. Package Management
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apt_packages: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub npm_packages: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pip_packages: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cargo_packages: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_linking: Option<PackageLinkingConfig>,

    // 9. Development Environment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal: Option<TerminalConfig>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub aliases: IndexMap<String, String>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub environment: IndexMap<String, String>,

    // 10. Feature Flags & Integrations
    #[serde(default, skip_serializing_if = "is_false")]
    pub claude_sync: bool,

    #[serde(default, skip_serializing_if = "is_false")]
    pub gemini_sync: bool,

    #[serde(default = "default_true")]
    pub copy_git_config: bool,

    // 11. Security
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security: Option<SecurityConfig>,

    // 12. Git Worktrees
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktrees: Option<WorktreesConfig>,

    // 13. Extra/Custom
    #[serde(flatten)]
    pub extra_config: IndexMap<String, serde_json::Value>,

    // 14. Internal-only fields
    /// Path to the config file that was loaded (for debugging)
    #[serde(skip)]
    pub source_path: Option<PathBuf>,

    /// Host Git configuration (if detected and enabled)
    #[serde(skip)]
    pub git_config: Option<GitConfig>,

    // 14. Mock provider config (for testing only)
    #[cfg(feature = "test-helpers")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mock: Option<MockProviderConfig>,
}

/// Configuration for the mock provider, for testing purposes.
#[cfg(feature = "test-helpers")]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MockProviderConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instances: Vec<MockVmInstanceConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_report: Option<VmStatusReportConfig>,
}

/// A mock VM instance for testing `vm list`.
#[cfg(feature = "test-helpers")]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MockVmInstanceConfig {
    pub name: String,
    pub status: String,
    pub ip_address: Option<String>,
    pub memory_gb: u32,
    pub cpus: u32,
}

/// A mock status report for testing `vm status`.
#[cfg(feature = "test-helpers")]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VmStatusReportConfig {
    pub name: String,
    pub is_running: bool,
    pub ip_address: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<(String, String)>,
}

/// Port configuration with range-based allocation and explicit mappings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PortsConfig {
    #[serde(rename = "_range", skip_serializing_if = "Option::is_none")]
    pub range: Option<Vec<u16>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mappings: Vec<PortMapping>,
}

impl PortsConfig {
    pub fn get_all_exposed_ports(&self) -> Vec<String> {
        let mut ports = Vec::new();

        // Add explicit mappings
        for mapping in &self.mappings {
            ports.push(format!("{}:{}", mapping.host, mapping.guest));
        }

        // Add range mapping
        if let Some(range) = &self.range {
            if range.len() == 2 {
                let (start, end) = (range[0], range[1]);
                ports.push(format!("{start}-{end}:{start}-{end}"));
            }
        }
        ports
    }

    pub fn has_ports(&self) -> bool {
        self.range.is_some() || !self.mappings.is_empty()
    }

    pub fn is_port_in_range(&self, port: u16) -> bool {
        if let Some(range) = &self.range {
            if range.len() == 2 {
                let (start, end) = (range[0], range[1]);
                return port >= start && port <= end;
            }
        }
        false
    }
}

/// Project-specific configuration settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_template_path: Option<String>,
}

/// Virtual machine resource and system configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VmSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub box_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<MemoryLimit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpus: Option<CpuLimit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swappiness: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port_binding: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gui: Option<bool>,
}

/// Memory limit configuration supporting both specific limits and unlimited access.
#[derive(Debug, Clone, PartialEq)]
pub enum MemoryLimit {
    /// Specific memory limit in megabytes
    Limited(u32),
    /// Percentage of available system memory (1-100)
    Percentage(u8),
    /// No memory limit
    Unlimited,
}

impl Serialize for MemoryLimit {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            MemoryLimit::Limited(mb) => serializer.serialize_u32(*mb),
            MemoryLimit::Percentage(percent) => {
                serializer.serialize_str(&format!("{}%", percent))
            }
            MemoryLimit::Unlimited => serializer.serialize_str("unlimited"),
        }
    }
}

impl<'de> Deserialize<'de> for MemoryLimit {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use crate::limit_parser::{LimitVisitor, ParsedLimit};
        use serde::de;

        let visitor = LimitVisitor::new("memory (MB)");
        let parsed = deserializer.deserialize_any(visitor)?;

        match parsed {
            ParsedLimit::Number(mb) => Ok(MemoryLimit::Limited(mb)),
            ParsedLimit::Bytes(bytes) => {
                let mb = (bytes / 1024 / 1024) as u32;
                Ok(MemoryLimit::Limited(mb))
            }
            ParsedLimit::Percentage(percent) => Ok(MemoryLimit::Percentage(percent)),
            ParsedLimit::Unlimited => Ok(MemoryLimit::Unlimited),
        }
    }
}

impl MemoryLimit {
    /// Get the memory limit in MB if it's a fixed value
    /// Returns None for Unlimited or Percentage (needs resolution)
    pub fn to_mb(&self) -> Option<u32> {
        match self {
            MemoryLimit::Limited(mb) => Some(*mb),
            MemoryLimit::Percentage(_) | MemoryLimit::Unlimited => None,
        }
    }

    /// Check if this limit is unlimited
    pub fn is_unlimited(&self) -> bool {
        matches!(self, MemoryLimit::Unlimited)
    }

    /// Check if this limit is a percentage
    pub fn is_percentage(&self) -> bool {
        matches!(self, MemoryLimit::Percentage(_))
    }

    /// Get the percentage value if this is a percentage limit
    pub fn to_percentage(&self) -> Option<u8> {
        match self {
            MemoryLimit::Percentage(percent) => Some(*percent),
            _ => None,
        }
    }

    /// Resolve a percentage limit to concrete MB value based on available memory
    pub fn resolve_percentage(&self, available_mb: u64) -> Option<u32> {
        match self {
            MemoryLimit::Percentage(percent) => {
                let mb = (available_mb * (*percent as u64) / 100) as u32;
                Some(mb)
            }
            MemoryLimit::Limited(mb) => Some(*mb),
            MemoryLimit::Unlimited => None,
        }
    }

    /// Convert to Docker memory format (e.g., "1024m")
    pub fn to_docker_format(&self) -> Option<String> {
        match self {
            MemoryLimit::Limited(mb) => Some(format!("{mb}m")),
            MemoryLimit::Percentage(_) | MemoryLimit::Unlimited => None,
        }
    }
}

/// CPU limit configuration supporting both specific limits and unlimited access.
#[derive(Debug, Clone, PartialEq)]
pub enum CpuLimit {
    /// Specific CPU count limit
    Limited(u32),
    /// Percentage of available CPUs (1-100)
    Percentage(u8),
    /// No CPU limit
    Unlimited,
}

impl Serialize for CpuLimit {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            CpuLimit::Limited(count) => serializer.serialize_u32(*count),
            CpuLimit::Percentage(percent) => serializer.serialize_str(&format!("{}%", percent)),
            CpuLimit::Unlimited => serializer.serialize_str("unlimited"),
        }
    }
}

impl<'de> Deserialize<'de> for CpuLimit {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use crate::limit_parser::{LimitVisitor, ParsedLimit};
        use serde::de;

        let visitor = LimitVisitor::new("CPU count");
        let parsed = deserializer.deserialize_any(visitor)?;

        match parsed {
            ParsedLimit::Number(count) => Ok(CpuLimit::Limited(count)),
            ParsedLimit::Bytes(_) => Err(de::Error::custom(
                "Memory units (gb, mb) are not valid for CPU limits",
            )),
            ParsedLimit::Percentage(percent) => Ok(CpuLimit::Percentage(percent)),
            ParsedLimit::Unlimited => Ok(CpuLimit::Unlimited),
        }
    }
}

impl CpuLimit {
    /// Get the CPU count if it's a fixed value
    /// Returns None for Unlimited or Percentage (needs resolution)
    pub fn to_count(&self) -> Option<u32> {
        match self {
            CpuLimit::Limited(count) => Some(*count),
            CpuLimit::Percentage(_) | CpuLimit::Unlimited => None,
        }
    }

    /// Check if this limit is unlimited
    pub fn is_unlimited(&self) -> bool {
        matches!(self, CpuLimit::Unlimited)
    }

    /// Check if this limit is a percentage
    pub fn is_percentage(&self) -> bool {
        matches!(self, CpuLimit::Percentage(_))
    }

    /// Get the percentage value if this is a percentage limit
    pub fn to_percentage(&self) -> Option<u8> {
        match self {
            CpuLimit::Percentage(percent) => Some(*percent),
            _ => None,
        }
    }

    /// Resolve a percentage limit to concrete CPU count based on available CPUs
    pub fn resolve_percentage(&self, available_cpus: u32) -> Option<u32> {
        match self {
            CpuLimit::Percentage(percent) => {
                let count = ((available_cpus * (*percent as u32)) / 100).max(1); // At least 1 CPU
                Some(count)
            }
            CpuLimit::Limited(count) => Some(*count),
            CpuLimit::Unlimited => None,
        }
    }
}

/// Language runtime and tool version specifications.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VersionsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub npm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pnpm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub python: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nvm: Option<String>,
}

/// Configuration for individual services and databases.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServiceConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_option_string_or_number"
    )]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buildx: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_microphone: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<u32>,

    // Per-project backup settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_on_destroy: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed_file: Option<PathBuf>,
}

/// Terminal and shell customization settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TerminalConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_git_branch: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_timestamp: Option<bool>,
}

/// Tart virtualization provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TartConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guest_os: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rosetta: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_docker: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_path: Option<String>,
}

/// Package linking and development workflow configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PackageLinkingConfig {
    #[serde(default)]
    pub npm: bool,
    #[serde(default)]
    pub pip: bool,
    #[serde(default)]
    pub cargo: bool,
}

fn is_false(b: &bool) -> bool {
    !b
}
fn default_true() -> bool {
    true
}

/// Git worktree configuration settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreesConfig {
    #[serde(default = "default_worktrees_enabled")]
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_path: Option<String>,
}

impl Default for WorktreesConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            base_path: None,
        }
    }
}

fn default_worktrees_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityConfig {
    #[serde(default)]
    pub enable_debugging: bool,
    #[serde(default = "default_true")]
    pub no_new_privileges: bool,
    #[serde(default)]
    pub user_namespaces: bool,
    #[serde(default)]
    pub read_only_root: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drop_capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub security_opts: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_limit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_limit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pids_limit: Option<u32>,
}

/// Docker networking configuration for container connectivity.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkingConfig {
    /// List of Docker networks this container should join.
    /// Networks will be created automatically if they don't exist.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub networks: Vec<String>,
}

impl VmConfig {
    pub fn load(file: Option<PathBuf>) -> Result<Self> {
        let mut config = crate::cli::load_and_merge_config(file)?;
        config.apply_default_backup_settings();
        Ok(config)
    }

    pub fn write_to_file(&self, path: &Path) -> Result<()> {
        let yaml = serde_yaml::to_string(self)?;
        fs::write(path, yaml)?;
        Ok(())
    }

    pub fn from_file(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(serde_yaml::from_str(&content)?)
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn apply_default_backup_settings(&mut self) {
        for (_, service) in self.services.iter_mut() {
            let should_backup = service.backup_on_destroy.is_none()
                && service.r#type.as_deref() == Some("database");

            if should_backup {
                service.backup_on_destroy = Some(true);
            }
        }
    }

    pub fn is_partial(&self) -> bool {
        self.provider.is_none() || self.project.as_ref().map_or(true, |p| p.name.is_none())
    }

    pub fn validate(&self, skip_port_availability_check: bool) -> Vec<String> {
        let mut errors = Vec::new();

        // Run the more comprehensive validation from the validate module.
        // This is a bit awkward as ConfigValidator returns a Result, not a Vec<String>.
        // We'll convert the error into a string for consistency with the rest of this method.
        let validator = crate::validate::ConfigValidator::new(
            self.clone(),
            std::path::PathBuf::new(),
            skip_port_availability_check,
        );
        if let Err(e) = validator.validate() {
            errors.push(e.to_string());
        }

        if let Some(provider) = &self.provider {
            #[cfg(feature = "test-helpers")]
            let valid_providers = ["docker", "vagrant", "tart", "mock"];
            #[cfg(not(feature = "test-helpers"))]
            let valid_providers = ["docker", "vagrant", "tart"];

            if !valid_providers.contains(&provider.as_str()) {
                errors.push(format!(
                    "Invalid provider '{}'. Valid providers are: {}",
                    provider,
                    valid_providers.join(", ")
                ));
            }
        }

        if let Some(vm) = &self.vm {
            if let Some(cpus) = &vm.cpus {
                if let Some(count) = cpus.to_count() {
                    #[allow(clippy::excessive_nesting)]
                    if count == 0 {
                        errors.push("VM CPU count cannot be 0".to_string());
                    }
                }
            }
            if let Some(memory) = &vm.memory {
                match memory.to_mb() {
                    Some(0) => {
                        errors.push("VM memory allocation cannot be 0".to_string());
                    }
                    Some(_) => {} // Valid memory allocation
                    None => {} // Unlimited memory is valid
                }
            }
        }

        for (service_name, service) in &self.services {
            if service.enabled && service.port.is_none() && service_name != "docker" {
                errors.push(format!(
                    "Service '{service_name}' is enabled but has no port specified"
                ));
            }
        }
        errors
    }

    pub fn ensure_service_ports(&mut self) {
        const PRIORITY_SERVICES: &[&str] = &["postgresql", "redis", "mysql", "mongodb"];
        const SERVICES_WITHOUT_PORTS: &[&str] = &["docker"];

        let range = match &self.ports.range {
            Some(r) if r.len() == 2 => r,
            _ => return,
        };
        let (range_start, range_end) = (range[0], range[1]);

        let mut used_ports: std::collections::HashSet<u16> =
            self.services.values().filter_map(|s| s.port).collect();

        let mut current_port = range_end;
        let mut get_next_port = || -> Option<u16> {
            while current_port >= range_start {
                let port = current_port;
                if current_port == range_start {
                    current_port = 0;
                } else {
                    current_port -= 1;
                }
                if !used_ports.contains(&port) {
                    used_ports.insert(port);
                    return Some(port);
                }
                if current_port == 0 {
                    break;
                }
            }
            None
        };

        let mut services_to_process = Vec::new();
        for &priority_service in PRIORITY_SERVICES {
            if let Some(service) = self.services.get(priority_service) {
                if service.enabled && service.port.is_none() {
                    services_to_process.push(priority_service.to_string());
                }
            }
        }

        let mut other_services: Vec<String> = self
            .services
            .iter()
            .filter(|(name, service)| {
                service.enabled
                    && service.port.is_none()
                    && !PRIORITY_SERVICES.contains(&name.as_str())
                    && !SERVICES_WITHOUT_PORTS.contains(&name.as_str())
            })
            .map(|(name, _)| name.clone())
            .collect();
        other_services.sort();
        services_to_process.extend(other_services);

        for service_name in services_to_process {
            if let Some(port) = get_next_port() {
                if let Some(service) = self.services.get_mut(&service_name) {
                    service.port = Some(port);
                }
            }
        }

        let disabled_services: Vec<String> = self
            .services
            .iter()
            .filter(|(_, service)| {
                !service.enabled
                    && service.port.is_some()
                    && service
                        .port
                        .is_some_and(|p| p >= range_start && p <= range_end)
            })
            .map(|(name, _)| name.clone())
            .collect();

        for service_name in disabled_services {
            if let Some(service) = self.services.get_mut(&service_name) {
                service.port = None;
            }
        }
    }
}
