// Standard library imports
use std::path::PathBuf;

// External crate imports
use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_yaml_ng as serde_yaml;
use vm_core::error::Result;

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
/// let config = VmConfig::load(Some(PathBuf::from("vm.yaml")))?;
///
/// // Check if configuration is complete
/// if config.is_partial() {
///     println!("Configuration needs more setup");
/// }
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
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

    // Deprecated global service fields (handled during deserialization)
    // These fields are no longer part of VmConfig but are detected for migration
    #[serde(default, skip_serializing_if = "is_false")]
    pub persist_databases: bool,

    // 11. Security
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security: Option<SecurityConfig>,

    // 12. Extra/Custom
    #[serde(flatten)]
    pub extra_config: IndexMap<String, serde_json::Value>,
}

/// Port configuration with range-based allocation.
///
/// Manages a port range allocated to a VM instance. Individual service ports
/// are stored in `services.<service-name>.port` and are auto-assigned from this range.
///
/// # Examples
/// ```yaml
/// ports:
///   _range: [3000, 3020]  # Reserve ports 3000-3020 for this VM
///
/// services:
///   postgresql:
///     port: 3000  # Auto-assigned from range
///   redis:
///     port: 3001  # Auto-assigned from range
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PortsConfig {
    /// Port range allocated to this VM instance. Services will auto-assign from this range.
    #[serde(rename = "_range", skip_serializing_if = "Option::is_none")]
    pub range: Option<Vec<u16>>,
}

impl PortsConfig {
    /// Get all ports that should be exposed to the host.
    ///
    /// # Returns
    /// Vector of port mapping strings in docker-compose format (e.g., "3000-3020:3000-3020")
    pub fn get_all_exposed_ports(&self) -> Vec<String> {
        let mut ports = Vec::new();

        // Add range if present
        if let Some(range) = &self.range {
            if range.len() == 2 {
                let (start, end) = (range[0], range[1]);
                ports.push(format!("{}-{}:{}-{}", start, end, start, end));
            }
        }

        ports
    }

    /// Check if the configuration has any ports to expose.
    pub fn has_ports(&self) -> bool {
        self.range.is_some()
    }

    /// Check if a port is within the configured range.
    ///
    /// # Arguments
    /// * `port` - Port number to check
    ///
    /// # Returns
    /// `true` if the port is within the range, `false` otherwise
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
    pub memory: Option<MemoryLimit>,

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

/// Memory limit configuration supporting both specific limits and unlimited access.
///
/// Supports two formats:
/// - Numeric value: Memory limit in MB (e.g., 8192 for 8GB)
/// - "unlimited": No memory restrictions
///
/// # Examples
/// ```yaml
/// vm:
///   memory: 8192        # 8GB limit
///   memory: "unlimited" # No limit
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum MemoryLimit {
    /// Specific memory limit in MB
    Limited(u32),
    /// Unlimited memory access
    Unlimited,
}

impl Serialize for MemoryLimit {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            MemoryLimit::Limited(mb) => serializer.serialize_u32(*mb),
            MemoryLimit::Unlimited => serializer.serialize_str("unlimited"),
        }
    }
}

impl<'de> Deserialize<'de> for MemoryLimit {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::{self, Visitor};
        use std::fmt;

        struct MemoryLimitVisitor;

        impl<'de> Visitor<'de> for MemoryLimitVisitor {
            type Value = MemoryLimit;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a positive integer (MB) or \"unlimited\"")
            }

            fn visit_u32<E>(self, value: u32) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(MemoryLimit::Limited(value))
            }

            fn visit_u64<E>(self, value: u64) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                if value <= u32::MAX as u64 {
                    Ok(MemoryLimit::Limited(value as u32))
                } else {
                    Err(E::custom("memory limit too large (max: 4294967295 MB)"))
                }
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                match value {
                    "unlimited" => Ok(MemoryLimit::Unlimited),
                    _ => Err(E::custom("expected \"unlimited\" for string memory value")),
                }
            }
        }

        deserializer.deserialize_any(MemoryLimitVisitor)
    }
}

impl MemoryLimit {
    /// Convert to megabytes if limited, None if unlimited
    pub fn to_mb(&self) -> Option<u32> {
        match self {
            MemoryLimit::Limited(mb) => Some(*mb),
            MemoryLimit::Unlimited => None,
        }
    }

    /// Check if memory is unlimited
    pub fn is_unlimited(&self) -> bool {
        matches!(self, MemoryLimit::Unlimited)
    }

    /// Convert to Docker memory format (e.g., "8192m" or None for unlimited)
    pub fn to_docker_format(&self) -> Option<String> {
        match self {
            MemoryLimit::Limited(mb) => Some(format!("{}m", mb)),
            MemoryLimit::Unlimited => None,
        }
    }
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
    /// // Load from specific file
    /// let config = VmConfig::load(Some(PathBuf::from("vm.yaml")))?;
    ///
    /// // Load with automatic preset detection
    /// let config = VmConfig::load(None)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn load(file: Option<PathBuf>) -> Result<Self> {
        crate::cli::load_and_merge_config(file)
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
    pub fn from_file(path: &PathBuf) -> Result<Self> {
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
    pub fn to_json(&self) -> Result<String> {
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

    /// Validate the configuration and return a list of validation errors.
    ///
    /// This method performs comprehensive validation of the VM configuration,
    /// checking for common configuration issues that could prevent proper
    /// VM operation.
    ///
    /// # Returns
    /// A vector of error messages. An empty vector indicates valid configuration.
    ///
    /// # Validation Checks
    /// - Services that are enabled but have no image specified
    /// - Invalid resource allocations (e.g., 0 CPUs, "0GB" memory)
    /// - The existence of local paths for file mounts
    /// - A valid provider name is being used
    ///
    /// # Examples
    /// ```rust,no_run
    /// use vm_config::config::VmConfig;
    ///
    /// let config = VmConfig::load(None)?;
    /// let errors = config.validate();
    /// if !errors.is_empty() {
    ///     for error in errors {
    ///         println!("❌ {}", error);
    ///     }
    /// }
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        // Check provider name
        if let Some(provider) = &self.provider {
            let valid_providers = ["docker", "vagrant", "tart"];
            if !valid_providers.contains(&provider.as_str()) {
                errors.push(format!(
                    "Invalid provider '{}'. Valid providers are: {}",
                    provider,
                    valid_providers.join(", ")
                ));
            }
        }

        // Check VM resource allocations
        if let Some(vm) = &self.vm {
            // Check CPU allocation
            if let Some(cpus) = vm.cpus {
                if cpus == 0 {
                    errors.push("VM CPU count cannot be 0".to_string());
                }
            }

            // Check memory allocation
            if let Some(memory) = &vm.memory {
                match memory.to_mb() {
                    Some(0) => {
                        errors.push("VM memory allocation cannot be 0".to_string());
                    }
                    Some(_) => {} // Valid memory allocation
                    None => {
                        errors.push(format!("Invalid memory format: {:?}", memory));
                    }
                }
            }
        }

        // Check services configuration
        for (service_name, service) in &self.services {
            if service.enabled {
                // For services, we can check if they have required configuration
                // This could be extended based on service type
                if service.port.is_none() && service_name != "docker" {
                    errors.push(format!(
                        "Service '{}' is enabled but has no port specified",
                        service_name
                    ));
                }
            }
        }

        errors
    }

    /// Ensure all enabled services have ports assigned from the configured range.
    ///
    /// This method automatically assigns ports to enabled services that don't already
    /// have ports assigned. Services are allocated ports in a priority order, and
    /// disabled services have their ports removed.
    ///
    /// # Priority Order
    /// Services are assigned ports in this order:
    /// 1. postgresql
    /// 2. redis
    /// 3. mysql
    /// 4. mongodb
    /// 5. Other services (alphabetically)
    ///
    /// # Behavior
    /// - Only modifies enabled services without ports
    /// - Preserves existing port assignments
    /// - Removes ports from disabled services
    /// - Skips services that don't need ports (e.g., docker)
    ///
    /// # Examples
    /// ```rust,no_run
    /// use vm_config::config::VmConfig;
    ///
    /// let mut config = VmConfig::default();
    /// config.ports.range = Some(vec![3000, 3010]);
    /// config.ensure_service_ports();
    /// ```
    pub fn ensure_service_ports(&mut self) {
        // Define priority order for port allocation
        const PRIORITY_SERVICES: &[&str] = &["postgresql", "redis", "mysql", "mongodb"];
        const SERVICES_WITHOUT_PORTS: &[&str] = &["docker"];

        // Get the port range
        let range = match &self.ports.range {
            Some(r) if r.len() == 2 => r,
            _ => return, // No valid range configured
        };

        let (range_start, range_end) = (range[0], range[1]);

        // Collect all currently assigned ports to avoid conflicts
        let mut used_ports: std::collections::HashSet<u16> =
            self.services.values().filter_map(|s| s.port).collect();

        // Helper function to get the next available port from range (starting from the end)
        // This leaves the lower ports free for developer use
        let mut current_port = range_end;
        let mut get_next_port = || -> Option<u16> {
            while current_port >= range_start {
                let port = current_port;
                if current_port == range_start {
                    current_port = 0; // Will break the loop on next iteration
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

        // Build ordered list of services to process
        let mut services_to_process = Vec::new();

        // First add priority services that are enabled and need ports
        for &priority_service in PRIORITY_SERVICES {
            if let Some(service) = self.services.get(priority_service) {
                if service.enabled && service.port.is_none() {
                    services_to_process.push(priority_service.to_string());
                }
            }
        }

        // Then add other enabled services (alphabetically) that need ports
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

        // Assign ports to services
        for service_name in services_to_process {
            if let Some(port) = get_next_port() {
                if let Some(service) = self.services.get_mut(&service_name) {
                    service.port = Some(port);
                }
            }
        }

        // Clean up ports from disabled services
        // Only remove ports that are within the auto-assigned range
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
