# Proposal 15: Fix Critical Installation Issues

**Priority:** P0 - BLOCKER
**Complexity:** Medium
**Estimated Time:** 4-6 hours

---

## Problem

**Onboarding is completely broken.** Test review shows 1/10 score with multiple critical failures:

1. ❌ `cargo install vm` fails (crate is a library, not a binary)
2. ❌ `vm init` generates invalid `vm.yaml` (missing `provider` and `project` fields)
3. ❌ Installation takes 10+ minutes (target: <3 minutes)
4. ❌ No helpful Docker error messages

---

## Specific Implementation Tasks

### Task 1: Fix `cargo install vm`
**File:** `rust/vm/Cargo.toml`

**Current (BROKEN):**
```toml
[package]
name = "vm"
publish = false  # ← This prevents publishing to crates.io
```

**Fix:**
1. Publish the `vm` CLI crate to crates.io
2. Update version to 2.1.0
3. Test installation: `cargo install vm`

**Acceptance:**
```bash
cargo install vm  # Must succeed in < 3 minutes
vm --version      # Must show installed version
```

---

### Task 2: Fix `vm init` Output
**File:** `rust/vm/src/commands/init.rs`

**Current (BROKEN):**
```yaml
# vm init generates this invalid config:
resources:
  cpus: 4
  memory: 8192
```

**Fix to Generate:**
```yaml
provider: docker

project:
  name: my-project

resources:
  cpus: 4
  memory: 8192
```

**Code Change:**
```rust
pub fn run(args: InitArgs) -> Result<()> {
    let vm_name = detect_project_name()?;
    let framework = detect_framework()?;

    let config = VmConfig {
        provider: Some("docker".to_string()),  // ← ADD THIS
        project: Some(ProjectConfig {          // ← ADD THIS
            name: vm_name.clone(),
        }),
        resources: Some(ResourceConfig {
            cpus: 4,
            memory: 8192,
        }),
        framework: framework,
        // ... rest
    };

    config.write_to_file("vm.yaml")?;
    println!("✓ Generated vm.yaml for {}", vm_name);
    Ok(())
}
```

**Acceptance:**
```bash
vm init
cat vm.yaml  # Must have provider and project fields
vm create    # Must succeed without errors
```

---

### Task 3: Add Better Docker Error Detection
**File:** `rust/vm-provider/src/docker/mod.rs`

Detect and fix common Docker issues:

```rust
pub fn validate_docker_environment() -> Result<()> {
    // Check 1: Docker installed
    if !Command::new("docker").arg("--version").status()?.success() {
        return Err(VmError::DockerNotInstalled(
            "Install from: https://docs.docker.com/get-docker/"
        ));
    }

    // Check 2: Docker daemon running
    if !Command::new("docker").arg("ps").status()?.success() {
        return Err(VmError::DockerNotRunning(
            "Start Docker Desktop or run: sudo systemctl start docker"
        ));
    }

    // Check 3: Docker permissions
    let output = Command::new("docker").arg("ps").output()?;
    if output.stderr.contains(b"permission denied") {
        return Err(VmError::DockerPermission(
            "Fix: sudo usermod -aG docker $USER && newgrp docker"
        ));
    }

    Ok(())
}
```

**Call Before VM Operations:**
```rust
// In create.rs, start.rs, etc.
pub fn run(args: CreateArgs) -> Result<()> {
    validate_docker_environment()?;  // ← Add this first
    // ... rest of command
}
```

**Acceptance:**
```bash
# Test with Docker stopped
sudo systemctl stop docker
vm create
# Should show: "Docker daemon is not running\nStart Docker Desktop..."

# Test with permission issue
vm create  # As non-docker user
# Should show: "Permission denied\nFix: sudo usermod -aG docker $USER"
```

---

### Task 4: Improve Installation Script
**File:** `install.sh`

**Add Clear Status Messages:**

```bash
install_from_cargo() {
    echo "📦 Installing VM tool from crates.io..."
    echo "⏱️  This may take 2-3 minutes..."

    if ! cargo install vm; then
        echo "❌ Installation failed"
        echo ""
        echo "Common fixes:"
        echo "  • Ensure Rust is up to date: rustup update"
        echo "  • Check internet connection"
        echo "  • Try: cargo install vm --locked"
        exit 1
    fi

    echo "✅ VM tool installed successfully"
}
```

**Acceptance:**
```bash
./install.sh
# Should show progress messages
# Should complete in < 3 minutes
# Should show clear errors if fails
```

---

### Task 5: Add Onboarding Validation Command
**File:** `rust/vm/src/commands/doctor.rs` (new file)

```rust
/// Check that VM tool is correctly set up
pub fn run(_args: DoctorArgs) -> Result<()> {
    println!("🔍 Running diagnostics...\n");

    // Check 1: Rust
    check_rust()?;

    // Check 2: Docker
    check_docker()?;

    // Check 3: VM binary
    check_vm_binary()?;

    // Check 4: Config
    check_config()?;

    println!("\n✅ All checks passed! VM tool is ready.");
    Ok(())
}

fn check_docker() -> Result<()> {
    print!("  Docker installed... ");
    if Command::new("docker").arg("--version").status()?.success() {
        println!("✓");
    } else {
        println!("❌");
        return Err(VmError::DockerNotInstalled);
    }

    print!("  Docker running... ");
    if Command::new("docker").arg("ps").status()?.success() {
        println!("✓");
    } else {
        println!("❌");
        return Err(VmError::DockerNotRunning);
    }

    Ok(())
}
```

**Add to CLI:**
```rust
#[derive(Subcommand)]
pub enum Commands {
    Doctor(DoctorArgs),  // ← Add this
    // ... rest
}
```

**Acceptance:**
```bash
vm doctor
# Should check all prerequisites
# Should show ✓ or ❌ for each check
# Should suggest fixes for failures
```

---

## Testing

```bash
# Test 1: Clean install
cargo uninstall vm
cargo install vm
vm --version  # Must succeed

# Test 2: Init command
mkdir /tmp/test-project && cd /tmp/test-project
echo '{"name":"test"}' > package.json
vm init
cat vm.yaml  # Must have provider and project fields

# Test 3: Create from init
vm create  # Must succeed without errors
vm status  # Must show running

# Test 4: Doctor command
vm doctor  # Must show all green checks

# Test 5: Error messages
sudo systemctl stop docker
vm create  # Must show helpful Docker error
```

---

## Success Criteria

- [ ] `cargo install vm` succeeds in < 3 minutes
- [ ] `vm init` generates valid config (has `provider` and `project`)
- [ ] `vm create` works on first try with valid config
- [ ] `vm doctor` validates all prerequisites
- [ ] Docker errors show specific fix instructions
- [ ] Can complete full onboarding in < 15 minutes
- [ ] Test review score improves from 1/10 to ≥7/10

---

## Priority Order

1. **Fix `vm init`** (highest impact, quickest fix)
2. **Add `vm doctor`** (helps users self-diagnose)
3. **Improve Docker errors** (reduces frustration)
4. **Publish to crates.io** (fixes `cargo install`)
5. **Update install.sh** (improves messaging)

Estimated total time: **4-6 hours**
