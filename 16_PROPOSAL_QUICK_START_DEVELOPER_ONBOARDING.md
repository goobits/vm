# Proposal 16: Improve Quick Start Experience

**Priority:** P1
**Complexity:** Medium
**Estimated Time:** 4-6 hours

---

## Problem

Jules' test review (see `ONBOARDING_REVIEW.md`) scored **1/10** with complete failure to create a VM.

**Critical Issues:**
1. Default resource allocation too high (8GB RAM, 4 CPUs)
2. `vm create` fails without `vm init` first
3. No detection of Docker permission issues
4. Rate limiting errors from Docker Hub not handled

---

## Specific Implementation Tasks

### Task 1: Add Sensible Resource Defaults
**File:** `rust/vm-config/src/lib.rs`

**Current (BROKEN):**
```rust
// Hardcoded defaults that don't fit most systems
const DEFAULT_MEMORY: u64 = 8192;  // 8GB - too much!
const DEFAULT_CPUS: u32 = 4;       // 4 cores - too many!
```

**Fix:**
```rust
pub fn detect_resource_defaults() -> ResourceConfig {
    let sys = System::new_all();

    let total_memory = sys.total_memory() / 1024;  // KB to MB
    let total_cpus = sys.cpus().len() as u32;

    // Use 50% of system resources, with minimums
    ResourceConfig {
        memory: std::cmp::max(2048, total_memory / 2),  // Min 2GB, max 50%
        cpus: std::cmp::max(2, total_cpus / 2),          // Min 2, max 50%
    }
}
```

**Acceptance:**
```bash
# On 16GB machine with 8 cores:
vm create  # Should allocate 8GB RAM, 4 CPUs (50%)

# On 4GB machine with 2 cores:
vm create  # Should allocate 2GB RAM, 2 CPUs (minimum)
```

---

### Task 2: Auto-Generate Config on First `vm create`
**File:** `rust/vm/src/commands/create.rs`

**Current:** Fails if no `vm.yaml` exists

**Fix:**
```rust
pub fn run(args: CreateArgs) -> Result<()> {
    let config_path = Path::new("vm.yaml");

    let config = if config_path.exists() {
        // Load existing config
        VmConfig::load_from_file(config_path)?
    } else {
        // Auto-generate if missing
        println!("📝 No vm.yaml found, generating default config...");

        let default_config = VmConfig {
            provider: Some("docker".to_string()),
            project: Some(ProjectConfig {
                name: detect_project_name()?,
            }),
            resources: Some(detect_resource_defaults()),
            framework: detect_framework().ok(),
            ..Default::default()
        };

        // Write it for next time
        default_config.write_to_file(config_path)?;
        println!("✓ Generated vm.yaml");

        default_config
    };

    // Continue with VM creation
    create_vm(&config, &args)?;
    Ok(())
}
```

**Acceptance:**
```bash
# Without vm.yaml
cd /tmp/test-project
echo '{"name":"test"}' > package.json
vm create  # Should auto-generate config and succeed
ls vm.yaml # Should exist now
```

---

### Task 3: Handle Docker Rate Limiting
**File:** `rust/vm-provider/src/docker/mod.rs`

**Current:** Fails with cryptic error on rate limit

**Fix:**
```rust
pub fn pull_image(image: &str) -> Result<()> {
    let output = Command::new("docker")
        .args(&["pull", image])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Detect rate limiting
        if stderr.contains("toomanyrequests") || stderr.contains("rate limit") {
            return Err(VmError::DockerRateLimit(format!(
                "Docker Hub rate limit reached\n\n\
                Fixes:\n\
                  • Wait 6 hours and try again\n\
                  • Login to Docker Hub: docker login\n\
                  • Use a different base image in vm.yaml"
            )));
        }

        return Err(VmError::DockerPullFailed(stderr.to_string()));
    }

    Ok(())
}
```

**Acceptance:**
```bash
# Simulate rate limit (use proxy or wait for natural limit)
vm create
# Should show: "Docker Hub rate limit reached\n\nFixes:\n  • Wait 6 hours..."
```

---

### Task 4: Add `--force` Flag to Skip Validations
**File:** `rust/vm/src/commands/create.rs`

**Add Flag:**
```rust
#[derive(Parser)]
pub struct CreateArgs {
    /// Skip resource validation and use defaults
    #[arg(long)]
    pub force: bool,

    // ... rest
}
```

**Use Flag:**
```rust
pub fn run(args: CreateArgs) -> Result<()> {
    // Load/generate config
    let mut config = load_or_generate_config()?;

    // If force, override with minimal resources
    if args.force {
        println!("⚡ Force mode: using minimal resources");
        config.resources = Some(ResourceConfig {
            memory: 2048,
            cpus: 2,
        });
    }

    // Validate unless force
    if !args.force {
        validate_config(&config)?;
    }

    create_vm(&config, &args)?;
    Ok(())
}
```

**Acceptance:**
```bash
# On low-resource machine
vm create --force  # Should use minimal 2GB/2CPU and succeed
```

---

### Task 5: Improve First-Run Experience Messages
**File:** `rust/vm/src/commands/create.rs`

**Add Helpful First-Run Messages:**
```rust
pub fn run(args: CreateArgs) -> Result<()> {
    // Check if this is first VM for this project
    let is_first_vm = !Path::new(".vm").exists();

    if is_first_vm {
        println!("👋 Creating your first VM for this project\n");
        println!("💡 Tip: Run 'vm init' first to customize resources");
        println!("⏱️  This may take 2-3 minutes...\n");
    }

    // ... rest of creation

    if is_first_vm {
        println!("\n🎉 Success! Your VM is ready");
        println!("📝 Next steps:");
        println!("  • ssh into VM:  vm ssh");
        println!("  • Run commands: vm exec 'npm install'");
        println!("  • View status:  vm status");
    }

    Ok(())
}
```

**Acceptance:**
```bash
vm create  # First time
# Should show welcome message, tips, next steps
```

---

## Testing

```bash
# Test 1: Auto-config generation
cd /tmp/fresh-project
echo '{"name":"test"}' > package.json
vm create  # Should auto-generate vm.yaml and succeed
cat vm.yaml  # Should have provider and project

# Test 2: Resource detection
vm create  # Should use 50% of system resources
vm status  # Should show allocated CPUs/memory

# Test 3: Force mode
vm create --force  # Should use minimal resources

# Test 4: Rate limit handling
# (Simulate by blocking Docker Hub or waiting for natural limit)
vm create  # Should show helpful rate limit message

# Test 5: First-run messages
rm -rf .vm vm.yaml
vm create  # Should show welcome, tips, next steps
```

---

## Success Criteria

- [ ] `vm create` works without requiring `vm init` first
- [ ] Auto-detects sensible resource limits (50% of system)
- [ ] `--force` flag uses minimal resources for low-spec machines
- [ ] Docker rate limit shows actionable error message
- [ ] First-run experience includes helpful tips
- [ ] Can create VM on 4GB/2-core machine
- [ ] Reduces onboarding time from 15+ min to < 5 min

---

## Priority Order

1. **Auto-generate config** (removes `vm init` requirement)
2. **Resource detection** (fixes allocation errors)
3. **First-run messages** (improves UX)
4. **Rate limit handling** (better errors)
5. **Force flag** (escape hatch for edge cases)

Estimated total time: **4-6 hours**
