# Proposal: Tart Provider Feature Parity with Docker

## Status
**Proposed** - Not yet implemented

## Executive Summary

Bring the Tart provider (macOS-native VMs via Apple Virtualization Framework) to feature parity with Docker by implementing 6 critical missing capabilities. This will elevate Tart from **30% advanced feature coverage to 100%**, making it a first-class option for macOS development environments, especially on Apple Silicon (M1/M2/M3).

**Current State**: ⭐⭐⭐ (85% core, 30% advanced) - Basic support with critical gaps
**Target State**: ⭐⭐⭐⭐⭐ (100% core, 100% advanced) - Full Docker parity

---

## Problem Statement

The Tart provider currently has **6 critical gaps** compared to Docker:

### Gap Analysis

| Feature | Docker | Tart | Impact |
|---------|--------|------|--------|
| **Provisioning Support** | ✅ Ansible | ❌ Shows "not supported" | **CRITICAL**: Cannot automate VM setup |
| **Enhanced Status Reports** | ✅ Full | ❌ Returns error | No CPU/memory/disk metrics or service monitoring |
| **TempProvider Support** | ✅ Full | ❌ Not supported | `vm temp` workflow completely unavailable |
| **ProviderContext Support** | ✅ Regenerates config | ❌ Ignores context | Config changes require manual `vm destroy && vm create` |
| **SSH Path Handling** | ✅ Changes to path | ❌ Ignores `relative_path` | **BROKEN**: Always connects to root directory |
| **Force Kill** | ✅ Distinct operation | ⚠️ Calls `stop()` | Can't force-kill hung VMs |

### Severity Classification

**🔴 Critical (Blocks Basic Usage)**:
1. ❌ **No provisioning** - VMs are essentially blank without manual setup
2. ❌ **SSH ignores path** - Breaks `vm ssh` expected behavior

**🟡 High (Missing Advanced Features)**:
3. ❌ **No enhanced status** - Can't monitor VM health
4. ❌ **No ProviderContext** - Config updates require destroy/recreate
5. ❌ **No TempProvider** - Temporary VM workflow unavailable

**🟢 Medium (Quality of Life)**:
6. ⚠️ **No force kill** - Can't forcefully terminate hung VMs

### User Impact

**Without these features, Tart users cannot:**
1. **Provision VMs automatically** (must manually install PostgreSQL, Redis, etc.)
2. **Use `vm ssh` correctly** (always lands in / instead of project directory)
3. Monitor VM resource usage (no dashboard metrics)
4. Apply config changes without losing all data
5. Use temporary VMs for quick testing
6. Force-kill stuck VMs

**Business Case**: macOS developers (especially on Apple Silicon) need native VMs for performance, but current Tart implementation is too limited for production use. This blocks adoption for macOS-centric teams.

---

## Proposed Solution

Implement 6 features across 6 incremental PRs to achieve full Docker parity.

### PR #1: Fix SSH Path Handling 🔴 CRITICAL (~30 LOC)

**Priority**: **Highest** - This is a broken core feature

**Current Behavior**:
```rust
fn ssh(&self, container: Option<&str>, relative_path: &Path) -> Result<()> {
    let instance_name = self.resolve_instance_name(container)?;

    // BUG: relative_path is completely ignored!
    let _ = relative_path;

    cmd!("tart", "ssh", &instance_name)
        .run()
        .map_err(|e| VmError::Provider(format!("SSH failed: {}", e)))?;

    Ok(())
}
```

**Fixed Implementation**:
```rust
fn ssh(&self, container: Option<&str>, relative_path: &Path) -> Result<()> {
    use duct::cmd;

    let instance_name = self.resolve_instance_name(container)?;

    // Get the sync directory (project root in VM)
    let sync_dir = self.get_sync_directory();

    // Resolve full path in VM
    let target_path = if relative_path == Path::new("") || relative_path == Path::new(".") {
        sync_dir.clone()
    } else {
        format!("{}/{}", sync_dir.trim_end_matches('/'), relative_path.display())
    };

    info!("Opening SSH session in directory: {}", target_path);

    // Use `tart ssh` with explicit cd command
    let ssh_command = format!("cd '{}' && exec $SHELL -l", target_path);

    cmd!("tart", "ssh", &instance_name, "--", "sh", "-c", &ssh_command)
        .run()
        .map_err(|e| VmError::Provider(format!("SSH failed: {}", e)))?;

    Ok(())
}
```

**Benefits**:
- `vm ssh` works correctly, changes to project directory
- Matches Docker and Vagrant behavior
- Fixes broken core functionality

**Estimated Effort**: 1-2 hours

---

### PR #2: Provisioning Support 🔴 CRITICAL (~250 LOC)

**Priority**: **Critical** - Blocks automated VM setup

**Current Behavior**:
```rust
fn provision(&self, container: Option<&str>) -> Result<()> {
    vm_println!("{}", MESSAGES.provision_not_supported);
    Ok(())
}
```

**Implementation Strategy**: Use **SSH + Shell Scripts** (simpler than Ansible for Tart)

```rust
// rust/vm-provider/src/tart/provisioner.rs (NEW FILE)

use vm_core::error::{Result, VmError};
use duct::cmd;
use std::path::Path;

pub struct TartProvisioner {
    instance_name: String,
    project_dir: String,
}

impl TartProvisioner {
    pub fn new(instance_name: String, project_dir: String) -> Self {
        Self {
            instance_name,
            project_dir,
        }
    }

    /// Run provisioning scripts over SSH
    pub fn provision(&self, config: &VmConfig) -> Result<()> {
        info!("Starting Tart VM provisioning for {}", self.instance_name);

        // 1. Wait for VM to be ready
        self.wait_for_ssh()?;

        // 2. Detect framework and install dependencies
        self.provision_framework_dependencies(config)?;

        // 3. Run custom provision scripts if present
        self.run_custom_provision_scripts(config)?;

        // 4. Start services
        self.start_services(config)?;

        info!("Provisioning completed successfully");
        Ok(())
    }

    fn wait_for_ssh(&self) -> Result<()> {
        use std::thread;
        use std::time::Duration;

        info!("Waiting for SSH to be ready...");

        for attempt in 1..=30 {
            let result = cmd!("tart", "ssh", &self.instance_name, "--", "echo", "ready")
                .stderr_null()
                .stdout_null()
                .run();

            if result.is_ok() {
                info!("SSH is ready");
                return Ok(());
            }

            thread::sleep(Duration::from_secs(2));
        }

        Err(VmError::Provider("SSH not ready after 60 seconds".to_string()))
    }

    fn provision_framework_dependencies(&self, config: &VmConfig) -> Result<()> {
        // Detect framework from vm.yaml
        let framework = self.detect_framework(config)?;

        info!("Detected framework: {}", framework);

        // Install framework-specific dependencies
        match framework.as_str() {
            "nodejs" => self.provision_nodejs(config)?,
            "python" => self.provision_python(config)?,
            "ruby" => self.provision_ruby(config)?,
            "rust" => self.provision_rust(config)?,
            "go" => self.provision_go(config)?,
            _ => {
                warn!("Unknown framework: {}, skipping framework provisioning", framework);
            }
        }

        // Install databases if configured
        self.provision_databases(config)?;

        Ok(())
    }

    fn detect_framework(&self, config: &VmConfig) -> Result<String> {
        // Check vm.yaml for explicit framework
        if let Some(framework) = &config.framework {
            return Ok(framework.clone());
        }

        // Auto-detect from project files
        let detection_script = r#"
            if [ -f "package.json" ]; then echo "nodejs"
            elif [ -f "requirements.txt" ] || [ -f "pyproject.toml" ]; then echo "python"
            elif [ -f "Gemfile" ]; then echo "ruby"
            elif [ -f "Cargo.toml" ]; then echo "rust"
            elif [ -f "go.mod" ]; then echo "go"
            else echo "unknown"
            fi
        "#;

        let output = self.ssh_exec(&format!("cd {} && {}", self.project_dir, detection_script))?;
        Ok(output.trim().to_string())
    }

    fn provision_nodejs(&self, config: &VmConfig) -> Result<()> {
        info!("Installing Node.js dependencies");

        // Install Node.js via nvm
        let node_version = config.runtime_version.as_deref().unwrap_or("20");

        let install_script = format!(r#"
            # Install nvm if not present
            if ! command -v nvm &> /dev/null; then
                curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash
                export NVM_DIR="$HOME/.nvm"
                [ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh"
            fi

            # Install Node.js
            nvm install {}
            nvm use {}

            # Install npm packages if package.json exists
            if [ -f {}/package.json ]; then
                cd {} && npm install
            fi
        "#, node_version, node_version, self.project_dir, self.project_dir);

        self.ssh_exec(&install_script)?;
        Ok(())
    }

    fn provision_python(&self, config: &VmConfig) -> Result<()> {
        info!("Installing Python dependencies");

        let python_version = config.runtime_version.as_deref().unwrap_or("3.11");

        let install_script = format!(r#"
            # Install pyenv if not present
            if ! command -v pyenv &> /dev/null; then
                curl https://pyenv.run | bash
                export PATH="$HOME/.pyenv/bin:$PATH"
                eval "$(pyenv init -)"
            fi

            # Install Python version
            pyenv install -s {}
            pyenv global {}

            # Install pip packages
            if [ -f {}/requirements.txt ]; then
                cd {} && pip install -r requirements.txt
            fi
        "#, python_version, python_version, self.project_dir, self.project_dir);

        self.ssh_exec(&install_script)?;
        Ok(())
    }

    fn provision_ruby(&self, config: &VmConfig) -> Result<()> {
        info!("Installing Ruby dependencies");

        let install_script = format!(r#"
            # Install rbenv if not present
            if ! command -v rbenv &> /dev/null; then
                git clone https://github.com/rbenv/rbenv.git ~/.rbenv
                cd ~/.rbenv && src/configure && make -C src
                export PATH="$HOME/.rbenv/bin:$PATH"
                eval "$(rbenv init -)"
            fi

            # Install bundler and gems
            if [ -f {}/Gemfile ]; then
                cd {} && gem install bundler && bundle install
            fi
        "#, self.project_dir, self.project_dir);

        self.ssh_exec(&install_script)?;
        Ok(())
    }

    fn provision_rust(&self, config: &VmConfig) -> Result<()> {
        info!("Installing Rust toolchain");

        let install_script = r#"
            # Install rustup if not present
            if ! command -v rustup &> /dev/null; then
                curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
                source $HOME/.cargo/env
            fi
        "#;

        self.ssh_exec(install_script)?;
        Ok(())
    }

    fn provision_go(&self, config: &VmConfig) -> Result<()> {
        info!("Installing Go");

        let go_version = config.runtime_version.as_deref().unwrap_or("1.21");

        let install_script = format!(r#"
            # Download and install Go
            wget -q https://go.dev/dl/go{}.linux-amd64.tar.gz
            sudo rm -rf /usr/local/go
            sudo tar -C /usr/local -xzf go{}.linux-amd64.tar.gz
            rm go{}.linux-amd64.tar.gz
            export PATH=$PATH:/usr/local/go/bin
        "#, go_version, go_version, go_version);

        self.ssh_exec(&install_script)?;
        Ok(())
    }

    fn provision_databases(&self, config: &VmConfig) -> Result<()> {
        // Check if databases are needed
        let services = config.services.as_ref();

        if services.map(|s| s.postgres.unwrap_or(false)).unwrap_or(false) {
            self.install_postgresql()?;
        }

        if services.map(|s| s.redis.unwrap_or(false)).unwrap_or(false) {
            self.install_redis()?;
        }

        if services.map(|s| s.mongodb.unwrap_or(false)).unwrap_or(false) {
            self.install_mongodb()?;
        }

        Ok(())
    }

    fn install_postgresql(&self) -> Result<()> {
        info!("Installing PostgreSQL");

        let install_script = r#"
            sudo apt-get update
            sudo apt-get install -y postgresql postgresql-contrib
            sudo systemctl enable postgresql
            sudo systemctl start postgresql
        "#;

        self.ssh_exec(install_script)?;
        Ok(())
    }

    fn install_redis(&self) -> Result<()> {
        info!("Installing Redis");

        let install_script = r#"
            sudo apt-get update
            sudo apt-get install -y redis-server
            sudo systemctl enable redis-server
            sudo systemctl start redis-server
        "#;

        self.ssh_exec(install_script)?;
        Ok(())
    }

    fn install_mongodb(&self) -> Result<()> {
        info!("Installing MongoDB");

        let install_script = r#"
            sudo apt-get update
            sudo apt-get install -y mongodb
            sudo systemctl enable mongodb
            sudo systemctl start mongodb
        "#;

        self.ssh_exec(install_script)?;
        Ok(())
    }

    fn run_custom_provision_scripts(&self, config: &VmConfig) -> Result<()> {
        // Check for provision.sh or similar scripts
        let script_path = format!("{}/provision.sh", self.project_dir);

        let check_script = format!(r#"
            if [ -f {} ]; then
                echo "found"
            fi
        "#, script_path);

        let output = self.ssh_exec(&check_script)?;

        if output.trim() == "found" {
            info!("Running custom provision script");
            self.ssh_exec(&format!("cd {} && bash provision.sh", self.project_dir))?;
        }

        Ok(())
    }

    fn start_services(&self, config: &VmConfig) -> Result<()> {
        info!("Starting configured services");

        // Services are started by systemctl in install functions
        // This is a no-op for Tart (unlike Docker where we start services here)

        Ok(())
    }

    fn ssh_exec(&self, command: &str) -> Result<String> {
        let output = cmd!("tart", "ssh", &self.instance_name, "--", "bash", "-c", command)
            .read()
            .map_err(|e| VmError::Provider(format!("SSH command failed: {}", e)))?;

        Ok(output)
    }
}
```

**Update provider.rs**:
```rust
// rust/vm-provider/src/tart/provider.rs

mod provisioner;
use provisioner::TartProvisioner;

impl Provider for TartProvider {
    fn provision(&self, container: Option<&str>) -> Result<()> {
        let instance_name = self.resolve_instance_name(container)?;

        // Create provisioner
        let provisioner = TartProvisioner::new(
            instance_name.clone(),
            self.get_sync_directory(),
        );

        // Run provisioning
        provisioner.provision(&self.config)?;

        vm_println!("{}", MESSAGES.provision_success);
        Ok(())
    }
}
```

**Benefits**:
- Automated framework detection (Node.js, Python, Ruby, Rust, Go)
- Automatic database installation (PostgreSQL, Redis, MongoDB)
- Custom provision scripts support
- Matches Docker provisioning capabilities

**Estimated Effort**: 8-10 hours

---

### PR #3: Enhanced Status Reports (batched SSH, ~200 LOC)

**Goal**: Implement `get_status_report()` for real-time metrics using a single SSH round-trip

**Implementation**:
```rust
// rust/vm-provider/src/tart/provider.rs

fn get_status_report(&self, container: Option<&str>) -> Result<VmStatusReport> {
    let instance_name = self.resolve_instance_name(container)?;

    if !self.is_instance_running(&instance_name)? {
        return Ok(VmStatusReport {
            name: instance_name.clone(),
            provider: "tart".into(),
            is_running: false,
            ..Default::default()
        });
    }

    let metrics = self.collect_metrics(&instance_name)?;

    Ok(VmStatusReport {
        name: instance_name,
        provider: "tart".into(),
        container_id: None,
        is_running: true,
        uptime: metrics.uptime,
        resources: metrics.resources,
        services: metrics.services,
    })
}

struct CollectedMetrics {
    resources: ResourceUsage,
    services: Vec<ServiceStatus>,
    uptime: Option<String>,
}

fn collect_metrics(&self, instance: &str) -> Result<CollectedMetrics> {
    use duct::cmd;

    let metrics_script = include_str!("scripts/collect_metrics.sh");
    let output = cmd!("tart", "ssh", instance, "--", "sh", "-c", metrics_script)
        .stderr_capture()
        .read()
        .map_err(|e| VmError::Provider(format!("SSH command failed: {}", e)))?;

    parse_metrics_json(&output).map_err(|e| VmError::Provider(format!("Failed to parse metrics: {}", e)))
}

fn parse_metrics_json(raw: &str) -> Result<CollectedMetrics> {
    #[derive(Deserialize)]
    struct Payload {
        cpu_percent: Option<f64>,
        memory_used_mb: Option<u64>,
        memory_limit_mb: Option<u64>,
        disk_used_gb: Option<f64>,
        disk_total_gb: Option<f64>,
        uptime: Option<String>,
        services: Vec<ServiceEntry>,
    }

    #[derive(Deserialize)]
    struct ServiceEntry {
        name: String,
        is_running: bool,
    }

    let payload: Payload = serde_json::from_str(raw)?;

    let resources = ResourceUsage {
        cpu_percent: payload.cpu_percent,
        memory_used_mb: payload.memory_used_mb,
        memory_limit_mb: payload.memory_limit_mb,
        disk_used_gb: payload.disk_used_gb,
        disk_total_gb: payload.disk_total_gb,
    };

    let services = payload
        .services
        .into_iter()
        .map(|svc| ServiceStatus {
            name: svc.name,
            is_running: svc.is_running,
            port: None,
            host_port: None,
            metrics: None,
            error: None,
        })
        .collect();

    Ok(CollectedMetrics {
        resources,
        services,
        uptime: payload.uptime,
    })
}
```

The companion `collect_metrics.sh` script runs inside the Linux guest and emits JSON for CPU, memory, disk, uptime, and systemd service status in one SSH call. This keeps `vm status` fast (< 3s) and avoids macOS-specific tooling that would fail inside the VM.

**Benefits**:
- Real-time CPU, memory, disk metrics
- Service health monitoring
- Enhanced dashboard support

**Estimated Effort**: 4-5 hours

---

### PR #4: Full ProviderContext Support (~100 LOC)

**Goal**: Make context methods regenerate VM configuration

**Implementation**:
```rust
// rust/vm-provider/src/tart/provider.rs

fn create_with_context(&self, context: &ProviderContext) -> Result<()> {
    // Use context config if available, otherwise use self.config
    let effective_config = context.global_config.as_ref().unwrap_or(&self.config);

    // Store effective config for this creation
    self.apply_config_from_context(effective_config)?;

    // Now create with updated config
    self.create_internal()
}

fn create_instance_with_context(&self, instance_name: &str, context: &ProviderContext) -> Result<()> {
    let effective_config = context.global_config.as_ref().unwrap_or(&self.config);
    self.apply_config_from_context(effective_config)?;
    self.create_instance_internal(instance_name)
}

fn start_with_context(&self, container: Option<&str>, context: &ProviderContext) -> Result<()> {
    let instance_name = self.resolve_instance_name(container)?;

    // Apply config changes
    if let Some(global_config) = &context.global_config {
        info!("Applying config updates to Tart VM");
        self.apply_runtime_config(&instance_name, global_config)?;
    }

    // Start VM
    self.start(Some(&instance_name))
}

fn restart_with_context(&self, container: Option<&str>, context: &ProviderContext) -> Result<()> {
    let instance_name = self.resolve_instance_name(container)?;

    // Apply config changes before restart
    if let Some(global_config) = &context.global_config {
        info!("Applying config updates to Tart VM");
        self.apply_runtime_config(&instance_name, global_config)?;
    }

    // Restart VM
    self.restart(Some(&instance_name))
}

fn apply_config_from_context(&self, config: &VmConfig) -> Result<()> {
    // Store config for use during creation
    // This is a placeholder - actual implementation would update self.config
    Ok(())
}

fn apply_runtime_config(&self, instance: &str, config: &VmConfig) -> Result<()> {
    use duct::cmd;

    // Apply CPU and memory changes using `tart set`
    if let Some(cpus) = config.vm.as_ref().and_then(|v| v.cpus) {
        info!("Setting CPU count to {}", cpus);
        cmd!("tart", "set", instance, "--cpu", cpus.to_string())
            .run()
            .map_err(|e| VmError::Provider(format!("Failed to set CPU: {}", e)))?;
    }

    if let Some(memory) = config.vm.as_ref().and_then(|v| v.memory.as_ref()) {
        if let Some(memory_mb) = memory.to_mb() {
            info!("Setting memory to {}MB", memory_mb);
            cmd!("tart", "set", instance, "--memory", format!("{}", memory_mb))
                .run()
                .map_err(|e| VmError::Provider(format!("Failed to set memory: {}", e)))?;
        }
    }

    Ok(())
}
```

**Benefits**:
- Config updates apply without destroy/recreate
- CPU and memory can be adjusted on running VMs
- Matches Docker context behavior

**Estimated Effort**: 3-4 hours

---

### PR #5: TempProvider Support (~180 LOC)

**Goal**: Implement `TempProvider` trait for `vm temp` workflow

**Implementation**:
```rust
// rust/vm-provider/src/tart/provider.rs

impl TempProvider for TartProvider {
    fn update_mounts(&self, state: &TempVmState) -> Result<()> {
        // For Tart, we need to stop VM, update mounts via `tart set`, then restart
        info!("Updating mounts for Tart VM: {}", state.name);

        // Stop VM
        self.stop(Some(&state.name))?;

        // Update mounts
        self.recreate_with_mounts(state)?;

        Ok(())
    }

    fn recreate_with_mounts(&self, state: &TempVmState) -> Result<()> {
        use duct::cmd;

        // Clear existing mounts
        // Note: Tart doesn't have a direct "clear mounts" command
        // We need to remove and re-add the VM with new mounts

        // Add new mounts using `tart set`
        for mount in &state.mounts {
            let mount_arg = format!(
                "{}:{}",
                mount.host_path.display(),
                mount.guest_path.display()
            );

            info!("Adding mount: {}", mount_arg);

            cmd!("tart", "set", &state.name, "--dir", &mount_arg)
                .run()
                .map_err(|e| VmError::Provider(format!("Failed to add mount: {}", e)))?;
        }

        // Start VM with new mounts
        self.start(Some(&state.name))?;

        Ok(())
    }

    fn check_container_health(&self, container_name: &str) -> Result<bool> {
        if !self.is_instance_running(container_name)? {
            return Ok(false);
        }

        // Test SSH connectivity
        let ssh_test = cmd!("tart", "ssh", container_name, "--", "echo", "healthy")
            .stderr_null()
            .stdout_null()
            .run();

        Ok(ssh_test.is_ok())
    }

    fn is_container_running(&self, container_name: &str) -> Result<bool> {
        self.is_instance_running(container_name)
    }
}

impl Provider for TartProvider {
    // ... existing methods ...

    fn as_temp_provider(&self) -> Option<&dyn TempProvider> {
        Some(self)  // Changed from None
    }
}
```

**Benefits**:
- `vm temp` command works with Tart
- Temporary VM workflow available for macOS users
- Mount management supported

**Estimated Effort**: 5-6 hours

---

### PR #6: Proper Force Kill Implementation (~50 LOC)

**Goal**: Implement distinct `kill()` that force-kills VM processes

**Implementation**:
```rust
// rust/vm-provider/src/tart/provider.rs

fn kill(&self, container: Option<&str>) -> Result<()> {
    use duct::cmd;

    let instance_name = self.resolve_instance_name(container)?;

    warn!("Force killing Tart VM: {}", instance_name);

    if let Err(e) = self.stop(Some(&instance_name)) {
        warn!("Graceful stop failed: {}", e);
    } else {
        info!("VM stopped gracefully");
        return Ok(());
    }

    cmd!("tart", "stop", "--force", &instance_name)
        .run()
        .map_err(|e| VmError::Provider(format!("Failed to force stop VM: {}", e)))?;

    info!("Tart VM force-stopped via CLI");
    Ok(())
}
```

**Benefits**:
- Can force-kill hung VMs using supported Tart CLI
- Distinct from regular stop operation
- Matches Docker force-kill behavior without process-scanning hacks

**Estimated Effort**: 2-3 hours

---

## Implementation Plan

### Incremental Rollout (6 PRs)

| PR | Feature | LOC | Effort | Risk | Priority | Dependencies |
|----|---------|-----|--------|------|----------|--------------|
| 1 | **Fix SSH Path** | ~30 | 1-2h | Low | 🔴 Critical | None |
| 2 | **Provisioning** | ~250 | 8-10h | High | 🔴 Critical | PR #1 (SSH) |
| 3 | **Enhanced Status** | ~200 | 4-5h | Low | 🟡 High | None |
| 4 | **ProviderContext** | ~100 | 3-4h | Low | 🟡 High | None |
| 5 | **TempProvider** | ~180 | 5-6h | Medium | 🟡 High | None |
| 6 | **Force Kill** | ~50 | 2-3h | Low | 🟢 Medium | None |

**Total**: ~750 LOC, 23-30 hours (3-4 days)

### Recommended Order

**Phase 1 - Critical Fixes** (Fix broken functionality):
1. PR #1: SSH Path Handling (1-2h) - **HIGHEST PRIORITY**
2. PR #2: Provisioning Support (8-10h) - **CRITICAL FEATURE**

**Phase 2 - Advanced Features** (Enable full capabilities):
3. PR #3: Enhanced Status Reports (4-5h)
4. PR #4: ProviderContext Support (3-4h)
5. PR #5: TempProvider Support (5-6h)

**Phase 3 - Quality of Life** (Polish):
6. PR #6: Force Kill (2-3h)

---

## Testing Strategy

**For Each PR**:
1. Unit tests for new methods
2. Integration tests with real Tart VMs on macOS
3. Test on both Intel and Apple Silicon Macs
4. Verify backward compatibility

**Test Coverage Targets**:
- PR #1: Test SSH lands in correct directory
- PR #2: Test provisioning for Node.js, Python, Ruby, Rust, Go + databases
- PR #3: Test status reports with real metrics
- PR #4: Test config updates without destroy
- PR #5: Test temp VM mount updates
- PR #6: Test force kill on hung VMs

**Platform Requirements**:
- macOS 13+ (Ventura or later)
- Apple Silicon (M1/M2/M3) or Intel
- Tart CLI installed (`brew install cirruslabs/cli/tart`)

---

## Risks and Mitigations

### Risk 1: Provisioning Complexity
**Risk**: PR #2 requires framework detection and package installation
**Mitigation**: Start with simple shell scripts, add frameworks incrementally (Node.js first, then Python, etc.)

### Risk 2: SSH Performance for Status
**Risk**: PR #3 requires multiple metrics; naive approach would be slow
**Mitigation**: Batch collection via `collect_metrics.sh`, consider light caching if needed

### Risk 3: Mount Updates Require Restart
**Risk**: PR #5 mount updates require VM restart (downtime)
**Mitigation**: Document as expected behavior, ensure restart is fast

### Risk 4: Tart CLI Limitations
**Risk**: Tart may not support all needed configuration changes
**Mitigation**: Test `tart set` capabilities early, document limitations

### Risk 5: macOS-Specific Behavior
**Risk**: Code only works on macOS, can't be tested in CI/CD
**Mitigation**: Use conditional compilation, skip tests on non-macOS platforms

---

## Success Metrics

### Quantitative Goals
- ✅ Tart passes 100% of Docker provider test suite (macOS only)
- ✅ Provisioning installs Node.js/Python/Ruby successfully
- ✅ SSH lands in correct directory 100% of the time
- ✅ Enhanced status reports return data within 3 seconds (single SSH round-trip)
- ✅ Config updates work without destroy/recreate

### Qualitative Goals
- ✅ macOS developers can use Tart as primary provider
- ✅ No feature limitations compared to Docker
- ✅ Tart provider rated ⭐⭐⭐⭐⭐ (same as Docker)

---

## Future Enhancements (Out of Scope)

**Not required for parity, but could be added later:**

1. **Rosetta 2 Support**: Run x86_64 VMs on Apple Silicon
2. **Screen Sharing**: `tart run --graphics` integration
3. **Snapshot Support**: VM state snapshots for quick restore
4. **Network Configuration**: Custom network settings
5. **GPU Passthrough**: Metal GPU acceleration
6. **Clipboard Sharing**: Host-guest clipboard sync

---

## Appendix A: Feature Comparison (Before vs After)

| Feature | Before | After | Docker Parity |
|---------|--------|-------|---------------|
| **Provisioning** | ❌ Not supported | ✅ Framework detection + services | ✅ 100% |
| **SSH Path Handling** | ❌ Broken (ignores path) | ✅ Changes to directory | ✅ 100% |
| **Enhanced Status Reports** | ❌ Returns error | ✅ CPU/Memory/Disk/Services | ✅ 100% |
| **ProviderContext Support** | ❌ Ignores context | ✅ Applies config changes | ✅ 100% |
| **TempProvider Support** | ❌ Not implemented | ✅ Full support | ✅ 100% |
| **Force Kill** | ⚠️ Calls stop() | ✅ CLI force stop | ✅ 100% |
| **Multi-Instance** | ✅ Works | ✅ Works | ✅ 100% |
| **Logs** | ⚠️ File-based | ⚠️ File-based | ⚠️ Acceptable |
| **Overall Rating** | ⭐⭐⭐ (30% advanced) | ⭐⭐⭐⭐⭐ (100% advanced) | ✅ Full Parity |

---

## Appendix B: Code Organization

**New Files**:
- `rust/vm-provider/src/tart/provisioner.rs` (~250 LOC)
- `rust/vm-provider/src/tart/scripts/collect_metrics.sh` (batched metrics helper)

**Modified Files**:
- `rust/vm-provider/src/tart/provider.rs` (~500 LOC added)

**Tests**:
- `rust/vm-provider/src/tart/provider_tests.rs` (new file, ~400 LOC)
- `rust/vm-provider/src/tart/provisioner_tests.rs` (new file, ~200 LOC)

---

## Appendix C: Platform Compatibility

**Supported Platforms**:
- ✅ macOS 13+ (Ventura, Sonoma, Sequoia)
- ✅ Apple Silicon (M1, M2, M3, M4)
- ✅ Intel Macs

**VM Guest OS**:
- ✅ Ubuntu 22.04+ (recommended)
- ✅ Debian 11+
- ⚠️ macOS guests (limited, requires specific images)

**Tart Version Requirements**:
- Minimum: Tart 2.0+
- Recommended: Tart 2.10+ (latest features)

---

## Conclusion

This proposal provides a **clear, incremental path** to bring Tart from basic support to full Docker parity. With **6 PRs totaling ~750 LOC and 23-30 hours of work**, the Tart provider will:

1. ✅ **Fix critical bugs** (SSH path handling)
2. ✅ **Add essential features** (provisioning)
3. ✅ **Enable advanced capabilities** (status, context, temp VMs)
4. ✅ **Improve reliability** (force kill)

**Impact**: macOS developers (especially on Apple Silicon) will have a **native, performant VM provider** with zero feature limitations compared to Docker. This enables:
- Faster VM performance via Apple Virtualization Framework
- Better battery life (native vs. Docker Desktop)
- Seamless integration with macOS ecosystem

**Recommendation**: Approve and implement in priority order:
- **Phase 1 (Critical)**: PR #1 → PR #2 (9-12 hours)
- **Phase 2 (Advanced)**: PR #3 → PR #4 → PR #5 (12-15 hours)
- **Phase 3 (Polish)**: PR #6 (2-3 hours)

This delivers **immediate value** with critical fixes while building toward full parity incrementally.
