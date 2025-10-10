# Proposal: Improve Logging and Output

**Status:** 🟡 High Priority UX Issue
**Priority:** P1
**Complexity:** Medium
**Estimated Effort:** 2-3 days

---

## Problem Statement

The VM CLI output is cluttered with verbose structured logging data (e.g., `request{...}`, `span{...}`) that confuses users. Internal debug/trace information appears alongside user-facing messages, making the tool feel unpolished and difficult to use.

### Impact

This UX issue affects:
- ❌ **First impressions** - New users see cryptic debug output
- ❌ **Scripting/automation** - Hard to parse actual output from logs
- ❌ **Documentation** - Examples show messy output
- ❌ **Error messages** - Important errors buried in log noise
- ❌ **Professional appearance** - Tool feels unfinished

### Current Behavior

```bash
$ vm create
request{operation="create" vm_name="test-project"}: ▶ Creating VM...
request{operation="create" vm_name="test-project"}:provision{step=1}: Installing packages...
request{operation="create" vm_name="test-project"}:provision{step=1}:install_apt{package="ripgrep"}: ✓ Installed ripgrep
request{operation="create" vm_name="test-project"}:provision{step=2}:install_npm{package="prettier"}: ✓ Installed prettier
request{operation="create" vm_name="test-project"}: ✓ VM created successfully
```

**User reaction:** "What are all these `request{...}` things?"

### Expected Behavior

```bash
$ vm create
▶ Creating VM test-project...
  ✓ Container created
  ▶ Provisioning...
    ✓ Installed ripgrep
    ✓ Installed prettier
✓ VM created successfully
```

**With `--verbose` flag:**
```bash
$ vm create --verbose
[2025-10-10T12:34:56Z INFO  vm::commands::create] Starting VM creation
request{operation="create" vm_name="test-project"}: ▶ Creating VM...
[2025-10-10T12:34:57Z DEBUG vm_provider::docker] Running docker run...
request{operation="create" vm_name="test-project"}:provision{step=1}: Installing packages...
[2025-10-10T12:34:58Z TRACE vm_package_manager] Executing: apt-get install ripgrep
...
```

---

## Root Cause Analysis

The logging system is configured to output structured logs to stdout/stderr at all log levels, regardless of whether the user wants to see them.

### Current Logging Setup

**File:** `rust/vm-logging/src/lib.rs`

The current tracing subscriber likely outputs to stdout without filtering:
```rust
pub fn init_subscriber() {
    tracing_subscriber::fmt()
        .with_target(true)  // Shows module path
        .with_level(true)   // Shows log level
        .init();
}
```

**Problem:** All spans and events are printed, creating noise.

---

## Proposed Solution

### 1. Architecture Overview

Separate three output channels:

1. **User Output** - Clean, formatted messages (always visible)
2. **Structured Logs** - Debug/trace info (only with `--verbose`)
3. **Error Output** - Critical errors (always visible to stderr)

```
┌─────────────────────┐
│  VM Commands        │
└──────────┬──────────┘
           │
           ├─→ vm_println!()  ──→  stdout (clean, always shown)
           ├─→ info!()        ──→  log file + verbose stdout
           ├─→ debug!()       ──→  log file + verbose stdout
           ├─→ trace!()       ──→  log file + verbose stdout
           └─→ vm_error!()    ──→  stderr (always shown)
```

### 2. Implementation Plan

#### Step 1: Configure Logging Layers

**File:** `rust/vm-logging/src/lib.rs`

```rust
use tracing_subscriber::{
    fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
};
use std::io;

pub struct LogConfig {
    pub verbose: bool,
    pub log_file: Option<PathBuf>,
    pub json_output: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            verbose: false,
            log_file: Some(home_dir().join(".vm/vm.log")),
            json_output: false,
        }
    }
}

pub fn init_subscriber_with_config(config: LogConfig) {
    let env_filter = if config.verbose {
        EnvFilter::new("vm=debug,vm_provider=debug,vm_config=debug")
    } else {
        EnvFilter::new("vm=warn")
    };

    let mut layers = Vec::new();

    // Layer 1: File output (always enabled, all levels)
    if let Some(log_file) = config.log_file {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)
            .expect("Failed to open log file");

        let file_layer = fmt::layer()
            .with_writer(move || file.try_clone().unwrap())
            .with_ansi(false)
            .with_target(true)
            .with_level(true)
            .with_filter(EnvFilter::new("vm=trace"));

        layers.push(file_layer.boxed());
    }

    // Layer 2: Console output (only if verbose)
    if config.verbose {
        let stdout_layer = fmt::layer()
            .with_writer(io::stdout)
            .with_target(true)
            .with_level(true)
            .with_filter(env_filter);

        layers.push(stdout_layer.boxed());
    }

    tracing_subscriber::registry()
        .with(layers)
        .init();
}

// Simplified init for backwards compatibility
pub fn init_subscriber() {
    init_subscriber_with_config(LogConfig::default());
}
```

#### Step 2: Add Global `--verbose` Flag

**File:** `rust/vm-cli/src/lib.rs`

```rust
#[derive(Parser)]
#[command(name = "vm")]
pub struct Cli {
    /// Enable verbose logging (shows debug/trace output)
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}
```

#### Step 3: Initialize Logging with User Preference

**File:** `rust/vm/src/main.rs`

```rust
use vm_logging::{init_subscriber_with_config, LogConfig};

fn main() {
    let cli = Cli::parse();

    // Initialize logging based on user preferences
    init_subscriber_with_config(LogConfig {
        verbose: cli.verbose,
        ..Default::default()
    });

    if let Err(e) = run(cli) {
        vm_error!("{:#}", e);
        std::process::exit(1);
    }
}
```

#### Step 4: Clean Up Output Macros

**File:** `rust/vm-messages/src/lib.rs`

Ensure user-facing output bypasses the logging system:

```rust
/// Print user-facing output (always visible, no log formatting)
#[macro_export]
macro_rules! vm_println {
    ($($arg:tt)*) => {
        println!($($arg)*);
    };
}

/// Print error to stderr (always visible)
#[macro_export]
macro_rules! vm_error {
    ($($arg:tt)*) => {
        eprintln!("{} {}", "❌".red(), format!($($arg)*).red());
    };
}

/// Print warning (always visible)
#[macro_export]
macro_rules! vm_warn {
    ($($arg:tt)*) => {
        eprintln!("{} {}", "⚠️".yellow(), format!($($arg)*).yellow());
    };
}

/// Print success (always visible)
#[macro_export]
macro_rules! vm_success {
    ($($arg:tt)*) => {
        println!("{} {}", "✓".green(), format!($($arg)*).green());
    };
}
```

#### Step 5: Replace Structured Logs in Commands

**File:** `rust/vm/src/commands/create.rs` (example)

**Before:**
```rust
#[instrument(skip(config))]
pub fn run(args: CreateArgs) -> Result<()> {
    info!("Creating VM");  // This shows as: request{...}: Creating VM
    // ...
}
```

**After:**
```rust
pub fn run(args: CreateArgs) -> Result<()> {
    debug!("Starting VM creation for {}", vm_name);  // Only in log file/verbose
    vm_println!("▶ Creating VM {}...", vm_name);     // Always shown, clean

    // Internal operations use debug/trace
    debug!("Loading configuration from {}", config_path);
    trace!("Config contents: {:?}", config);

    // User-facing progress
    vm_println!("  ✓ Container created");
    vm_println!("  ▶ Provisioning...");

    // Success message
    vm_success!("VM created successfully");

    Ok(())
}
```

### 3. Log File Management

**File:** `rust/vm-logging/src/lib.rs`

Add log rotation and cleanup:

```rust
use std::fs;

const MAX_LOG_SIZE: u64 = 10 * 1024 * 1024; // 10 MB

fn setup_log_file(path: &Path) -> Result<fs::File> {
    // Create parent directory if needed
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Rotate log if too large
    if path.exists() {
        let metadata = fs::metadata(path)?;
        if metadata.len() > MAX_LOG_SIZE {
            let backup = path.with_extension("log.old");
            fs::rename(path, backup)?;
        }
    }

    Ok(fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?)
}
```

### 4. Add `vm logs` Command

**File:** `rust/vm/src/commands/logs.rs`

```rust
use clap::Parser;

#[derive(Parser)]
pub struct LogsArgs {
    /// Show last N lines
    #[arg(short = 'n', long, default_value = "100")]
    pub lines: usize,

    /// Follow log output
    #[arg(short, long)]
    pub follow: bool,
}

pub fn run(args: LogsArgs) -> Result<()> {
    let log_file = home_dir().join(".vm/vm.log");

    if !log_file.exists() {
        bail!("Log file not found at {}", log_file.display());
    }

    if args.follow {
        // Use tail -f equivalent
        follow_log(&log_file)?;
    } else {
        // Show last N lines
        show_tail(&log_file, args.lines)?;
    }

    Ok(())
}
```

---

## Testing Strategy

### Manual Testing

Test output with and without `--verbose`:

```bash
# Normal output (clean)
$ vm create
▶ Creating VM test-project...
✓ VM created successfully

# Verbose output (with debug info)
$ vm create --verbose
[2025-10-10T12:34:56Z INFO  vm::commands::create] Starting VM creation
[2025-10-10T12:34:56Z DEBUG vm_config] Loading config from /path/to/vm.yaml
▶ Creating VM test-project...
[2025-10-10T12:34:57Z DEBUG vm_provider::docker] Executing: docker run...
✓ VM created successfully

# Check log file
$ vm logs -n 20
[2025-10-10T12:34:56Z INFO  vm::commands::create] Starting VM creation
[2025-10-10T12:34:56Z DEBUG vm_config] Loading config from /path/to/vm.yaml
...
```

### Unit Tests

```rust
#[test]
fn test_logging_config() {
    let config = LogConfig {
        verbose: false,
        log_file: None,
        json_output: false,
    };
    // Test that verbose=false doesn't output debug logs
}

#[test]
fn test_log_rotation() {
    // Test that large log files are rotated
}
```

---

## Edge Cases to Handle

1. **Log file permissions**
   - Handle read-only log directory
   - Fall back to temp directory if needed

2. **Concurrent writes**
   - Multiple VM commands running simultaneously
   - Ensure log file locking/append is atomic

3. **Environment variable override**
   - `VM_LOG_LEVEL=trace` for emergency debugging
   - `VM_LOG_FILE=/custom/path.log`

4. **CI/CD environments**
   - Auto-enable verbose in non-TTY environments?
   - Or keep clean for CI output parsing

---

## Configuration Options

### Environment Variables

```bash
export VM_LOG_LEVEL=debug      # Override log level
export VM_LOG_FILE=/dev/null   # Disable file logging
export VM_VERBOSE=1            # Enable verbose by default
```

### Config File

`~/.vm/config.yaml`:
```yaml
logging:
  verbose: false
  log_file: ~/.vm/vm.log
  max_size: 10485760  # 10 MB
  keep_backups: 3
```

---

## Acceptance Criteria

- [ ] Default output is clean (no `request{...}` noise)
- [ ] `--verbose` flag shows all debug/trace logs
- [ ] All logs written to `~/.vm/vm.log` regardless of verbose mode
- [ ] User-facing macros (`vm_println!`, `vm_success!`) bypass logging
- [ ] Error messages always visible on stderr
- [ ] Log file auto-rotates when exceeding 10MB
- [ ] `vm logs` command to view log file
- [ ] `vm logs -f` to follow logs in real-time
- [ ] Environment variables override defaults
- [ ] No regression in error reporting
- [ ] E2E tests pass with clean output

---

## Documentation Updates

### User Guide

**Using Verbose Mode:**
```bash
# Show debug output for troubleshooting
vm create --verbose

# View recent logs
vm logs

# Follow logs in real-time
vm logs -f

# Show last 50 lines
vm logs -n 50
```

**Log File Location:**
- Linux/macOS: `~/.vm/vm.log`
- Windows: `%USERPROFILE%\.vm\vm.log`

### Developer Guide

**Using Output Macros:**
```rust
use vm_messages::{vm_println, vm_success, vm_error, vm_warn};

// User-facing output (always shown, clean)
vm_println!("▶ Starting operation...");
vm_success!("Operation completed");
vm_error!("Operation failed");
vm_warn!("Deprecated feature");

// Internal logging (only verbose/log file)
debug!("Debug info for developers");
trace!("Detailed trace information");
info!("General information");
error!("Internal error details");
```

---

## Timeline

- **Day 1:** Update logging configuration, add global `--verbose` flag
- **Day 2:** Replace structured logs with clean output macros in all commands
- **Day 3:** Add `vm logs` command, log rotation, testing, documentation

---

## Success Metrics

- ✅ Default output contains no `request{...}` or `span{...}` text
- ✅ `--verbose` flag shows detailed debug information
- ✅ All commands produce clean, professional output
- ✅ Log file contains full debug history
- ✅ Users report improved UX in feedback
- ✅ Documentation examples show clean output
