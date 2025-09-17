use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_yaml_ng as serde_yaml;
use std::path::PathBuf;

/// Main VM configuration structure.
///
/// This is the central configuration structure that defines all aspects of a VM setup,
/// including provider settings, project configuration, services, packages, and environment.
/// It supports serialization to/from YAML and JSON formats.
///
/// ## Configuration Sections
/// - **Project**: Basic project metadata and paths
/// - **VM Settings**: Resource allocation and VM-specific configuration
/// - **Services**: Database and service configurations
/// - **Packages**: Language-specific package installations
/// - **Environment**: Shell aliases and environment variables
/// - **Provider-specific**: Docker, Vagrant, or Tart configurations
///
/// ## Usage
/// This structure is typically loaded from YAML files and merged from multiple sources:
/// 1. Default configuration
/// 2. Global user configuration
/// 3. Preset configuration (auto-detected)
/// 4. Project-specific configuration
///
/// # Examples
/// ```rust,no_run
/// use vm_config::config::VmConfig;
/// use std::path::PathBuf;
///
/// // Load configuration with auto-detection
/// let config = VmConfig::load(Some(PathBuf::from("vm.yaml")), false)?;
///
/// // Check if configuration is complete
/// if config.is_partial() {
///     println!("Configuration needs more setup");
/// }
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VmConfig {
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub vm: Option<VmSettings>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub versions: Option<VersionsConfig>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub ports: IndexMap<String, u16>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub port_range: Option<String>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub services: IndexMap<String, ServiceConfig>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apt_packages: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub npm_packages: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pip_packages: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cargo_packages: Vec<String>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub aliases: IndexMap<String, String>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub environment: IndexMap<String, String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal: Option<TerminalConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tart: Option<TartConfig>,

    #[serde(default, skip_serializing_if = "is_false")]
    pub claude_sync: bool,

    #[serde(default, skip_serializing_if = "is_false")]
    pub gemini_sync: bool,

    #[serde(default, skip_serializing_if = "is_false")]
    pub persist_databases: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_linking: Option<PackageLinkingConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub security: Option<SecurityConfig>,

    #[serde(flatten)]
    pub extra_config: IndexMap<String, serde_json::Value>,
}

/// Project-specific configuration settings.
///
/// Contains metadata and paths related to the project being developed,
/// including workspace configuration and backup settings.
///
/// # Fields
/// - `name`: Project identifier (used for VM naming)
/// - `hostname`: Custom hostname for the VM
/// - `workspace_path`: Path to the main workspace directory inside the VM
/// - `backup_pattern`: Glob pattern for files to backup
/// - `env_template_path`: Path to environment variable template file
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
///
/// Defines the VM's hardware resources, system settings, and virtualization options.
/// These settings control the VM's performance characteristics and system behavior.
///
/// # Resource Settings
/// - `memory`: RAM allocation in MB
/// - `cpus`: Number of CPU cores
/// - `swap`: Swap space in MB
/// - `swappiness`: Linux swap usage preference (0-100)
///
/// # System Settings
/// - `user`: Default user account in the VM
/// - `timezone`: System timezone configuration
/// - `gui`: Enable graphical interface support
/// - `port_binding`: Network port binding strategy
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VmSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub box_name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpus: Option<u32>,

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

/// Language runtime and tool version specifications.
///
/// Allows pinning specific versions of programming language runtimes and tools.
/// This ensures consistent development environments across different machines.
///
/// # Supported Tools
/// - `node`: Node.js runtime version
/// - `npm`: npm package manager version
/// - `pnpm`: pnpm package manager version
/// - `python`: Python interpreter version
/// - `nvm`: Node Version Manager version
///
/// # Examples
/// ```yaml
/// versions:
///   node: "18.17.0"
///   python: "3.11"
///   npm: "9.8.1"
/// ```
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
///
/// Defines how services (databases, caches, etc.) should be configured and deployed
/// within the VM. Each service can have its own specific settings and credentials.
///
/// # Common Fields
/// - `enabled`: Whether the service should be started
/// - `version`: Specific version to install/use
/// - `port`: Network port for the service
/// - `user`/`password`: Authentication credentials
///
/// # Service-Specific Fields
/// - **Database**: `database` name, connection settings
/// - **Docker**: `buildx` multi-platform build support
/// - **Browser**: `display`, `executable_path` for headless browsers
/// - **Audio**: `driver`, `share_microphone` for audio services
/// - **GPU**: `memory_mb` for GPU memory allocation
///
/// # Examples
/// ```yaml
/// services:
///   postgresql:
///     enabled: true
///     version: "15"
///     port: 5432
///     user: "dev"
///     password: "dev"
///     database: "myapp_dev"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServiceConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    // Database-specific fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,

    // Docker-specific fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buildx: Option<bool>,

    // Headless browser-specific fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable_path: Option<String>,

    // Audio-specific fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_microphone: Option<bool>,

    // GPU-specific fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<u32>,
}

/// Terminal and shell customization settings.
///
/// Controls the appearance and behavior of the terminal environment within the VM.
/// These settings help create a personalized and productive development experience.
///
/// # Appearance
/// - `shell`: Default shell program (bash, zsh, fish, etc.)
/// - `theme`: Color theme for the terminal
/// - `emoji`: Emoji style for prompts and output
///
/// # Prompt Configuration
/// - `username`: Display name in prompt
/// - `show_git_branch`: Show current Git branch in prompt
/// - `show_timestamp`: Include timestamp in prompt
///
/// # Examples
/// ```yaml
/// terminal:
///   shell: "zsh"
///   theme: "dark"
///   show_git_branch: true
///   show_timestamp: false
/// ```
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
///
/// Tart is a macOS virtualization tool that provides lightweight VMs on Apple Silicon.
/// This configuration section defines Tart-specific settings for VM creation and management.
///
/// # Core Settings
/// - `image`: Base image to use for the VM
/// - `guest_os`: Guest operating system type
/// - `disk_size`: Virtual disk size in GB
/// - `ssh_user`: Default SSH user account
///
/// # macOS-Specific
/// - `rosetta`: Enable Rosetta 2 for x86_64 emulation
/// - `storage_path`: Custom storage location for VM data
/// - `install_docker`: Automatically install Docker in the VM
///
/// # Examples
/// ```yaml
/// tart:
///   image: "ghcr.io/cirruslabs/macos-monterey-base:latest"
///   disk_size: 50
///   rosetta: true
///   ssh_user: "admin"
/// ```
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
///
/// Controls how package managers should handle local development dependencies
/// and workspace linking. This is particularly useful for monorepos and
/// projects with local package dependencies.
///
/// # Package Managers
/// - `npm`: Enable npm link/workspace functionality
/// - `pip`: Enable pip editable installs (-e flag)
/// - `cargo`: Enable Cargo workspace linking
///
/// The `npm` field defaults to `true` as it's commonly needed for JavaScript
/// development workflows. Other package managers default to `false` to avoid
/// unexpected behavior.
///
/// # Examples
/// ```yaml
/// package_linking:
///   npm: true    # Link local npm packages
///   pip: true    # Enable pip editable installs
///   cargo: false # Standard Cargo behavior
/// ```
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

/// Security configuration for Docker container isolation.
///
/// Controls security features that affect container isolation and host protection.
/// By default, development convenience is prioritized, but these options allow
/// hardening the container against escape attempts.
///
/// # Host Escape Prevention
/// - `enable_debugging`: Controls SYS_PTRACE and seccomp (default: true for dev)
/// - `no_new_privileges`: Prevents privilege escalation via SUID binaries
/// - `user_namespaces`: Remaps container UIDs to unprivileged host UIDs
///
/// # Container Hardening
/// - `read_only_root`: Makes root filesystem read-only (requires explicit mounts)
/// - `drop_capabilities`: List of capabilities to explicitly drop
/// - `security_opts`: Additional Docker security options
///
/// # Examples
/// ```yaml
/// # security:
/// #   enable_debugging: false    # Disable ptrace/seccomp for production
/// #   no_new_privileges: true    # Prevent privilege escalation
/// #   user_namespaces: true      # Enable UID remapping
/// ```
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

impl VmConfig {
    /// Load, merge, and validate configuration from an optional file path.
    ///
    /// This is the primary entry point for loading VM configuration. It handles
    /// the complete configuration loading pipeline including:
    /// 1. Loading user configuration from file
    /// 2. Auto-detecting and applying presets (unless disabled)
    /// 3. Merging with default and global configurations
    /// 4. Validating the final configuration
    ///
    /// # Arguments
    /// * `file` - Optional path to configuration file (searches for vm.yaml if None)
    /// * `no_preset` - If true, disables automatic preset detection
    ///
    /// # Returns
    /// A fully merged and validated `VmConfig` ready for use
    ///
    /// # Errors
    /// Returns an error if:
    /// - Configuration file cannot be read or parsed
    /// - Merged configuration is invalid
    /// - Required fields are missing after merging
    ///
    /// # Examples
    /// ```rust,no_run
    /// use vm_config::config::VmConfig;
    /// use std::path::PathBuf;
    ///
    /// // Load with auto-detection
    /// let config = VmConfig::load(Some(PathBuf::from("vm.yaml")), false)?;
    ///
    /// // Load without preset auto-detection
    /// let config = VmConfig::load(None, true)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn load(file: Option<PathBuf>, no_preset: bool) -> anyhow::Result<Self> {
        crate::cli::load_and_merge_config(file, no_preset)
    }

    /// Load configuration with a specific preset override.
    ///
    /// This allows forcing a specific preset regardless of auto-detection results.
    /// Useful when you want to use a particular preset that might not be
    /// automatically detected, or when overriding the detection logic.
    ///
    /// # Arguments
    /// * `file` - Optional path to configuration file
    /// * `preset_name` - Name of the preset to force (e.g., "react", "python")
    ///
    /// # Returns
    /// A fully merged and validated `VmConfig` with the specified preset applied
    ///
    /// # Errors
    /// Returns an error if:
    /// - The specified preset cannot be found
    /// - Configuration merging fails
    /// - Final configuration is invalid
    ///
    /// # Examples
    /// ```rust,no_run
    /// use vm_config::config::VmConfig;
    /// use std::path::PathBuf;
    ///
    /// // Force React preset
    /// let config = VmConfig::load_with_preset(
    ///     Some(PathBuf::from("vm.yaml")),
    ///     "react".to_string()
    /// )?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn load_with_preset(file: Option<PathBuf>, preset_name: String) -> anyhow::Result<Self> {
        crate::cli::load_and_merge_config_with_preset(file, preset_name)
    }

    /// Load configuration directly from a YAML file.
    ///
    /// This is a low-level loading function that reads and parses a single
    /// configuration file without any merging or preset detection. Use this
    /// when you need to load a specific configuration file without the
    /// full configuration pipeline.
    ///
    /// # Arguments
    /// * `path` - Path to the YAML configuration file
    ///
    /// # Returns
    /// A `VmConfig` struct parsed from the file
    ///
    /// # Errors
    /// Returns an error if:
    /// - File cannot be read
    /// - YAML parsing fails
    /// - Configuration structure is invalid
    ///
    /// # Examples
    /// ```rust,no_run
    /// use vm_config::config::VmConfig;
    /// use std::path::PathBuf;
    ///
    /// let config = VmConfig::from_file(&PathBuf::from("config.yaml"))?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn from_file(path: &PathBuf) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(serde_yaml::from_str(&content)?)
    }

    /// Convert configuration to a formatted JSON string.
    ///
    /// Serializes the configuration to JSON with pretty-printing for
    /// readability. Useful for debugging, API responses, or when JSON
    /// format is preferred over YAML.
    ///
    /// # Returns
    /// A pretty-formatted JSON string representation of the configuration
    ///
    /// # Errors
    /// Returns an error if serialization fails (rare)
    ///
    /// # Examples
    /// ```rust,no_run
    /// use vm_config::config::VmConfig;
    ///
    /// let config = VmConfig::default();
    /// let json = config.to_json()?;
    /// println!("Config: {}", json);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Check if this configuration is incomplete (missing required fields).
    ///
    /// A partial configuration is one that lacks essential fields needed for
    /// VM operation. This typically occurs with user configurations that are
    /// meant to be merged with defaults and presets.
    ///
    /// # Required Fields
    /// - `provider`: VM provider (docker, vagrant, tart)
    /// - `project.name`: Project identifier
    ///
    /// # Returns
    /// `true` if required fields are missing, `false` if configuration is complete
    ///
    /// # Examples
    /// ```rust,no_run
    /// use vm_config::config::VmConfig;
    ///
    /// let config = VmConfig::default();
    /// if config.is_partial() {
    ///     println!("Configuration needs to be merged with defaults");
    /// }
    /// ```
    pub fn is_partial(&self) -> bool {
        self.provider.is_none() || self.project.as_ref().map_or(true, |p| p.name.is_none())
    }
}
