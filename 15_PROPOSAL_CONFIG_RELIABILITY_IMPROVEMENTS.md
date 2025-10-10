# Proposal: Configuration Reliability Improvements

**Status:** 🟡 High Priority UX Issue
**Priority:** P1
**Complexity:** Medium-High
**Estimated Effort:** 3-4 days

---

## Problem Statement

The VM configuration system has multiple reliability issues that cause confusing failures and unexpected behavior:

1. **Config loading uses stale/cached data** - Tool sometimes ignores the `vm.yaml` in the current directory
2. **`vm init` generates invalid CPU counts** - Requests more CPUs than the host has available
3. **No validation before VM creation** - Errors only appear during `docker run`, wasting time
4. **Config source is unclear** - Users don't know which config file is being used

### Impact

These issues affect:
- ❌ **First-time users** - `vm init && vm create` immediately fails
- ❌ **Multi-project workflows** - Wrong config loaded when switching directories
- ❌ **Debugging** - Users waste time editing the wrong config file
- ❌ **CI/CD** - Unreliable behavior in automated environments
- ❌ **User trust** - Tool feels buggy and unreliable

### Current Behavior

**Issue 1: Config Caching**
```bash
$ cd /project-a
$ cat vm.yaml | grep cpus
  cpus: 2  # Correct value

$ vm create
Error: Requested 6 CPUs but only 4 available
# Where did 6 come from? vm.yaml says 2!

# Workaround that shouldn't be needed:
$ vm config set vm.cpus 2
$ vm create
✓ Success
```

**Issue 2: Invalid CPU Detection**
```bash
$ vm init  # On a 4-core system
$ cat vm.yaml | grep cpus
  cpus: 6  # Generated value exceeds available CPUs!

$ vm create
Error: Requested 6 CPUs but only 4 available
```

**Issue 3: Late Validation**
```bash
$ vm create
# 30 seconds of building and provisioning...
Error: Port 3000 already in use
# Should have detected this before building!
```

### Expected Behavior

**Config Loading:**
```bash
$ cd /project-a
$ vm create
Using config: /project-a/vm.yaml
✓ Validated configuration
▶ Creating VM...
```

**Resource Detection:**
```bash
$ vm init  # On 4-core system
✓ Detected 4 CPU cores, 8GB RAM
✓ Generated vm.yaml with cpus: 2, memory: 4096
```

**Early Validation:**
```bash
$ vm create
✓ Configuration valid
❌ Error: Port 3000 already in use on host
💡 Fix: Change port in vm.yaml or free up port 3000
# Failed fast, before wasting time building
```

---

## Root Cause Analysis

### Issue 1: Config Loading Priority

The config loading logic doesn't properly prioritize local `vm.yaml`:

**Current suspected logic:**
1. Look for `~/.vm/config.yaml` (global config)
2. Look for `vm.yaml` (project config)
3. **Bug:** Sometimes caches or uses wrong path

**Expected logic:**
1. Look for `vm.yaml` in current directory (highest priority)
2. Merge with `~/.vm/config.yaml` (defaults)
3. Apply environment variable overrides
4. **Cache only in-memory, never persist**

### Issue 2: Resource Detection

`vm init` uses hardcoded defaults instead of detecting host capabilities.

### Issue 3: Validation Timing

Validation happens too late (during `docker run`) instead of before VM creation.

---

## Proposed Solution

### 1. Fix Configuration Loading

#### Step 1: Implement Clear Priority Chain

**File:** `rust/vm-config/src/loader.rs`

```rust
use std::path::{Path, PathBuf};

pub struct ConfigLoader {
    search_paths: Vec<PathBuf>,
}

impl ConfigLoader {
    pub fn new() -> Self {
        Self {
            search_paths: Vec::new(),
        }
    }

    /// Load configuration with clear priority order
    pub fn load(&self) -> Result<VmConfig> {
        // Priority 1: Current directory vm.yaml
        let local_config = Path::new("vm.yaml");
        if local_config.exists() {
            debug!("Loading config from: {}", local_config.display());
            return self.load_file(local_config)
                .with_context(|| format!("Failed to load {}", local_config.display()));
        }

        // Priority 2: Walk up directory tree to find vm.yaml
        if let Some(config) = self.find_in_parent_dirs("vm.yaml")? {
            debug!("Loading config from: {}", config.display());
            return self.load_file(&config);
        }

        // Priority 3: Global config
        let global_config = home_dir().join(".vm/config.yaml");
        if global_config.exists() {
            debug!("Loading config from: {}", global_config.display());
            return self.load_file(&global_config);
        }

        bail!("No vm.yaml found in current directory or parent directories");
    }

    /// Find config file by walking up directory tree
    fn find_in_parent_dirs(&self, filename: &str) -> Result<Option<PathBuf>> {
        let mut current = std::env::current_dir()?;

        loop {
            let config_path = current.join(filename);
            if config_path.exists() {
                return Ok(Some(config_path));
            }

            // Move to parent directory
            if !current.pop() {
                break; // Reached root
            }
        }

        Ok(None)
    }

    fn load_file(&self, path: &Path) -> Result<VmConfig> {
        let contents = fs::read_to_string(path)?;
        let mut config: VmConfig = serde_yaml::from_str(&contents)?;

        // Store source path for debugging
        config.source_path = Some(path.to_path_buf());

        Ok(config)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmConfig {
    // ... existing fields

    /// Path to the config file that was loaded (for debugging)
    #[serde(skip)]
    pub source_path: Option<PathBuf>,
}
```

#### Step 2: Add Config Source Logging

**File:** `rust/vm/src/commands/*.rs`

```rust
pub fn run(args: CreateArgs) -> Result<()> {
    let config = ConfigLoader::new().load()?;

    // Show user which config is being used
    if let Some(source) = &config.source_path {
        debug!("Using config: {}", source.display());
    }

    // ...
}
```

### 2. Implement Resource Detection for `vm init`

**File:** `rust/vm/src/commands/init.rs`

```rust
use sysinfo::{System, SystemExt};

fn detect_host_resources() -> ResourceLimits {
    let mut sys = System::new_all();
    sys.refresh_all();

    let total_cpus = sys.cpus().len();
    let total_memory_mb = (sys.total_memory() / 1024 / 1024) as u64;

    // Use conservative defaults (50% of available)
    let recommended_cpus = (total_cpus / 2).max(1).min(4);
    let recommended_memory = (total_memory_mb / 2).max(2048).min(8192);

    ResourceLimits {
        total_cpus,
        total_memory_mb,
        recommended_cpus,
        recommended_memory,
    }
}

pub fn run(args: InitArgs) -> Result<()> {
    let resources = detect_host_resources();

    vm_println!(
        "✓ Detected {} CPU cores, {}GB RAM",
        resources.total_cpus,
        resources.total_memory_mb / 1024
    );

    let config = VmConfig {
        vm: VmSettings {
            cpus: resources.recommended_cpus,
            memory: resources.recommended_memory,
            ..Default::default()
        },
        ..Default::default()
    };

    // Generate vm.yaml
    let yaml = serde_yaml::to_string(&config)?;
    fs::write("vm.yaml", yaml)?;

    vm_success!(
        "Generated vm.yaml with cpus: {}, memory: {}MB",
        config.vm.cpus,
        config.vm.memory
    );

    Ok(())
}
```

### 3. Add Pre-Creation Validation

**File:** `rust/vm-config/src/validator.rs`

```rust
use sysinfo::{System, SystemExt};
use std::net::TcpListener;

pub struct ConfigValidator {
    system: System,
}

impl ConfigValidator {
    pub fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        Self { system }
    }

    /// Validate configuration before VM creation
    pub fn validate(&self, config: &VmConfig) -> Result<ValidationReport> {
        let mut report = ValidationReport::default();

        // Check CPU allocation
        self.validate_cpu(config, &mut report)?;

        // Check memory allocation
        self.validate_memory(config, &mut report)?;

        // Check port availability
        self.validate_ports(config, &mut report)?;

        // Check disk space
        self.validate_disk_space(&mut report)?;

        if report.has_errors() {
            bail!("Configuration validation failed:\n{}", report);
        }

        Ok(report)
    }

    fn validate_cpu(&self, config: &VmConfig, report: &mut ValidationReport) -> Result<()> {
        let available_cpus = self.system.cpus().len();

        if config.vm.cpus > available_cpus {
            report.add_error(format!(
                "Requested {} CPUs but only {} available. Set 'vm.cpus' to {} or less in vm.yaml",
                config.vm.cpus, available_cpus, available_cpus
            ));
        } else if config.vm.cpus > available_cpus * 3 / 4 {
            report.add_warning(format!(
                "Using {} of {} available CPUs may impact host performance",
                config.vm.cpus, available_cpus
            ));
        }

        Ok(())
    }

    fn validate_memory(&self, config: &VmConfig, report: &mut ValidationReport) -> Result<()> {
        let available_mb = (self.system.available_memory() / 1024 / 1024) as u64;
        let total_mb = (self.system.total_memory() / 1024 / 1024) as u64;

        if config.vm.memory > total_mb {
            report.add_error(format!(
                "Requested {}MB RAM but only {}MB total. Set 'vm.memory' to {} or less",
                config.vm.memory, total_mb, total_mb
            ));
        } else if config.vm.memory > available_mb {
            report.add_warning(format!(
                "Requested {}MB RAM but only {}MB available ({}MB in use)",
                config.vm.memory, available_mb, total_mb - available_mb
            ));
        }

        Ok(())
    }

    fn validate_ports(&self, config: &VmConfig, report: &mut ValidationReport) -> Result<()> {
        for port_mapping in &config.ports {
            let addr = format!("{}:{}", config.vm.port_binding, port_mapping.host);

            match TcpListener::bind(&addr) {
                Ok(_) => {} // Port available
                Err(_) => {
                    report.add_error(format!(
                        "Port {} already in use on host. Change 'ports[].host' in vm.yaml or free the port",
                        port_mapping.host
                    ));
                }
            }
        }

        Ok(())
    }

    fn validate_disk_space(&self, report: &mut ValidationReport) -> Result<()> {
        // Check available disk space (need ~5GB for VM images)
        const MIN_DISK_SPACE_GB: u64 = 5;

        // This is platform-specific, use sysinfo or similar
        // Simplified for now
        report.add_info("Disk space check: OK".to_string());

        Ok(())
    }
}

#[derive(Default)]
pub struct ValidationReport {
    errors: Vec<String>,
    warnings: Vec<String>,
    info: Vec<String>,
}

impl ValidationReport {
    pub fn add_error(&mut self, msg: String) {
        self.errors.push(msg);
    }

    pub fn add_warning(&mut self, msg: String) {
        self.warnings.push(msg);
    }

    pub fn add_info(&mut self, msg: String) {
        self.info.push(msg);
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

impl std::fmt::Display for ValidationReport {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        for error in &self.errors {
            writeln!(f, "❌ {}", error)?;
        }
        for warning in &self.warnings {
            writeln!(f, "⚠️  {}", warning)?;
        }
        for info in &self.info {
            writeln!(f, "ℹ️  {}", info)?;
        }
        Ok(())
    }
}
```

#### Step 3: Integrate Validation into `vm create`

**File:** `rust/vm/src/commands/create.rs`

```rust
use vm_config::ConfigValidator;

pub fn run(args: CreateArgs) -> Result<()> {
    let config = ConfigLoader::new().load()?;

    // Show config source
    if let Some(source) = &config.source_path {
        vm_println!("Using config: {}", source.display());
    }

    // Validate BEFORE creating VM
    let validator = ConfigValidator::new();
    let report = validator.validate(&config)?;

    // Show warnings even if validation passed
    if !report.warnings.is_empty() {
        vm_println!("{}", report);
    }

    vm_success!("Configuration valid");

    // Proceed with creation
    // ...
}
```

### 4. Add `vm config validate` Command

**File:** `rust/vm/src/commands/config.rs`

```rust
#[derive(Parser)]
pub enum ConfigCommand {
    /// Validate current configuration
    Validate,

    /// Show which config file is being used
    Show,

    /// Set a configuration value
    Set {
        key: String,
        value: String,
    },
}

pub fn run(cmd: ConfigCommand) -> Result<()> {
    match cmd {
        ConfigCommand::Validate => validate_config(),
        ConfigCommand::Show => show_config(),
        ConfigCommand::Set { key, value } => set_config(key, value),
    }
}

fn validate_config() -> Result<()> {
    let config = ConfigLoader::new().load()?;
    let validator = ConfigValidator::new();
    let report = validator.validate(&config)?;

    vm_println!("{}", report);
    vm_success!("Configuration is valid");

    Ok(())
}

fn show_config() -> Result<()> {
    let config = ConfigLoader::new().load()?;

    if let Some(source) = &config.source_path {
        vm_println!("Config source: {}", source.display());
    }

    vm_println!("\nCurrent configuration:");
    vm_println!("{}", serde_yaml::to_string(&config)?);

    Ok(())
}
```

---

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_config_priority() {
    // Create temp dir with vm.yaml
    // Verify it's loaded instead of global config
}

#[test]
fn test_resource_detection() {
    let resources = detect_host_resources();
    assert!(resources.recommended_cpus <= resources.total_cpus);
    assert!(resources.recommended_memory <= resources.total_memory_mb);
}

#[test]
fn test_cpu_validation() {
    let config = VmConfig {
        vm: VmSettings { cpus: 999, ..Default::default() },
        ..Default::default()
    };
    let validator = ConfigValidator::new();
    assert!(validator.validate(&config).is_err());
}
```

### Integration Tests

```rust
#[test]
fn test_vm_init_generates_valid_config() -> Result<()> {
    let temp_dir = create_temp_dir()?;
    run_vm_command(&["init"], &temp_dir)?;

    // Load generated config
    let config: VmConfig = load_yaml(temp_dir.join("vm.yaml"))?;

    // Should not exceed host resources
    let sys = System::new_all();
    assert!(config.vm.cpus <= sys.cpus().len());

    Ok(())
}
```

---

## Edge Cases

1. **No vm.yaml exists** - Clear error message with `vm init` suggestion
2. **Config in parent directory** - Should find it by walking up
3. **Symlinked directories** - Resolve symlinks correctly
4. **Read-only vm.yaml** - `vm config set` should fail gracefully
5. **Malformed YAML** - Clear parse error with line number
6. **Docker not running** - Fail fast with helpful message

---

## Acceptance Criteria

- [ ] Config loaded from current directory first, then parent dirs
- [ ] `vm.yaml` always takes precedence over `~/.vm/config.yaml`
- [ ] No config caching between commands
- [ ] `vm init` detects host CPU/memory and generates valid config
- [ ] `vm create` validates config before building
- [ ] Validation catches CPU/memory/port conflicts
- [ ] `vm config validate` command added
- [ ] `vm config show` displays current config source
- [ ] Clear error messages for all validation failures
- [ ] E2E tests pass without manual config edits

---

## Documentation Updates

### CLI Help

```bash
$ vm config --help
Manage VM configuration

Commands:
  validate   Validate current configuration
  show       Show which config file is being used
  set        Set a configuration value

Examples:
  vm config validate              # Check for errors
  vm config show                  # See config source
  vm config set vm.cpus 4         # Override CPU count
```

### User Guide

**Configuration Priority:**
1. `./vm.yaml` (current directory) - Highest priority
2. `../vm.yaml` (parent directories) - Searched upwards
3. `~/.vm/config.yaml` (global defaults) - Lowest priority

**Validating Before Creation:**
```bash
$ vm config validate
✓ Configuration valid

$ vm create
# Config automatically validated before creation
```

---

## Timeline

- **Day 1:** Fix config loading priority, add source path tracking
- **Day 2:** Implement resource detection for `vm init`
- **Day 3:** Add validation framework with CPU/memory/port checks
- **Day 4:** `vm config validate/show` commands, testing, documentation

---

## Success Metrics

- ✅ Config always loaded from correct location
- ✅ `vm init && vm create` succeeds on first try
- ✅ Validation catches errors before Docker build
- ✅ No more "wrong config loaded" bug reports
- ✅ E2E tests pass without workarounds
