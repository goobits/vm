# Proposal 14: Improve Logging and Output

**Priority:** P1
**Complexity:** Low
**Estimated Time:** 3-4 hours

---

## Problem

Users see cluttered output with `request{...}` and `span{...}` noise. Errors lack actionable suggestions.

---

## Specific Implementation Tasks

### Task 1: Hide Structured Logs by Default
**File:** `rust/vm-logging/src/lib.rs`

```rust
pub fn init_subscriber_with_config(verbose: bool) {
    let filter = if verbose {
        EnvFilter::new("vm=debug")
    } else {
        EnvFilter::new("vm=warn")  // Only errors by default
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(verbose)  // Hide targets unless verbose
        .with_level(verbose)   // Hide levels unless verbose
        .init();
}
```

**Acceptance:** `vm create` shows clean output, no `request{...}` visible

---

### Task 2: Add Global `--verbose` Flag
**File:** `rust/vm-cli/src/lib.rs`

```rust
#[derive(Parser)]
pub struct Cli {
    /// Show debug logs
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}
```

**Acceptance:** `vm create --verbose` shows all debug output

---

### Task 3: Replace Logging with User-Facing Output
**Files:** `rust/vm/src/commands/*.rs` (all command files)

Replace:
```rust
info!("Creating VM");  // Shows: request{...}: Creating VM
```

With:
```rust
debug!("Creating VM");              // Only in --verbose mode
println!("▶ Creating VM {}...", name);  // Always shown, clean
```

**Acceptance:** All commands use `println!` for user output, `debug!` for internal logs

---

### Task 4: Improve Error Messages
**File:** `rust/vm-core/src/error.rs`

Add contextual error display:

```rust
impl Display for VmError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            VmError::DockerNotRunning => {
                write!(f, "Docker daemon is not running\n\n")?;
                write!(f, "Fix:\n")?;
                write!(f, "  • Start Docker Desktop, or\n")?;
                write!(f, "  • Run: sudo systemctl start docker\n")?;
                write!(f, "  • Verify: docker ps")
            }
            VmError::DockerPermission => {
                write!(f, "Permission denied accessing Docker\n\n")?;
                write!(f, "Fix:\n")?;
                write!(f, "  • Add user to docker group: sudo usermod -aG docker $USER\n")?;
                write!(f, "  • Log out and back in\n")?;
                write!(f, "  • Or use: sudo vm create")
            }
            // Add similar for all error types
        }
    }
}
```

**Acceptance:** All errors include specific fix suggestions

---

### Task 5: Support CI/CD Mode
**File:** `rust/vm-cli/src/main.rs`

```rust
// Auto-detect CI environment
let is_ci = std::env::var("CI").is_ok();
let no_color = std::env::var("NO_COLOR").is_ok();

if is_ci {
    // Disable colors and interactive elements
    std::env::set_var("NO_COLOR", "1");
}
```

**Acceptance:** `CI=true vm list` produces clean, parseable output

---

## Testing Commands

```bash
# Test clean output
vm create
# Should show: ▶ Creating VM test-project...
# Should NOT show: request{operation="create"}

# Test verbose mode
vm create --verbose
# Should show debug logs with timestamps

# Test error messages
docker stop $(docker ps -q)  # Stop Docker
vm create
# Should show helpful error with fix suggestions

# Test CI mode
CI=true vm list
# Should have no colors, no progress bars
```

---

## Success Criteria

- [ ] Default output is clean (no `request{...}`)
- [ ] `--verbose` shows debug info
- [ ] All errors suggest specific fixes
- [ ] CI mode auto-detected and respected
- [ ] Takes < 4 hours to implement
