# Proposal: Tart Provider Feature Parity with Docker

## Status
**Proposed** - Not yet implemented

## Executive Summary

Bring the Tart provider (macOS-native VMs via Apple Virtualization Framework) to feature parity with Docker by implementing 6 critical missing capabilities in a single comprehensive PR. This will elevate Tart from **30% advanced feature coverage to 100%**, making it a first-class option for macOS development environments, especially on Apple Silicon (M1/M2/M3).

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

Implement all 6 features in a **single comprehensive PR** (~810 LOC) to achieve full Docker parity.

### Implementation Overview

The PR will add:
- **Provisioning system** with framework detection and service installation
- **Batched SSH metrics collection** for status reports
- **TempProvider trait implementation** for temporary VM support
- **ProviderContext handlers** for dynamic config updates
- **SSH path handling fix** for correct directory navigation
- **Force kill implementation** using Tart CLI

---

## Detailed Implementation

### 1. Fix SSH Path Handling 🔴 CRITICAL

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

---

### 2. Provisioning Support 🔴 CRITICAL

**Strategy**: SSH-based provisioning with framework detection and service installation

**New File**: `rust/vm-provider/src/tart/provisioner.rs`

```rust
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
        let framework = self.detect_framework(config)?;
        info!("Detected framework: {}", framework);

        match framework.as_str() {
            "nodejs" => self.provision_nodejs(config)?,
            "python" => self.provision_python(config)?,
            "ruby" => self.provision_ruby(config)?,
            "rust" => self.provision_rust(config)?,
            "go" => self.provision_go(config)?,
            _ => warn!("Unknown framework: {}, skipping", framework),
        }

        self.provision_databases(config)?;
        Ok(())
    }

    fn detect_framework(&self, config: &VmConfig) -> Result<String> {
        if let Some(framework) = &config.framework {
            return Ok(framework.clone());
        }

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
        let node_version = config.runtime_version.as_deref().unwrap_or("20");

        let install_script = format!(r#"
            if ! command -v nvm &> /dev/null; then
                curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash
                export NVM_DIR="$HOME/.nvm"
                [ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh"
            fi

            nvm install {}
            nvm use {}

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
            if ! command -v pyenv &> /dev/null; then
                curl https://pyenv.run | bash
                export PATH="$HOME/.pyenv/bin:$PATH"
                eval "$(pyenv init -)"
            fi

            pyenv install -s {}
            pyenv global {}

            if [ -f {}/requirements.txt ]; then
                cd {} && pip install -r requirements.txt
            fi
        "#, python_version, python_version, self.project_dir, self.project_dir);

        self.ssh_exec(&install_script)?;
        Ok(())
    }

    fn provision_databases(&self, config: &VmConfig) -> Result<()> {
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
        self.ssh_exec(r#"
            sudo apt-get update
            sudo apt-get install -y postgresql postgresql-contrib
            sudo systemctl enable postgresql
            sudo systemctl start postgresql
        "#)?;
        Ok(())
    }

    fn install_redis(&self) -> Result<()> {
        info!("Installing Redis");
        self.ssh_exec(r#"
            sudo apt-get update
            sudo apt-get install -y redis-server
            sudo systemctl enable redis-server
            sudo systemctl start redis-server
        "#)?;
        Ok(())
    }

    fn install_mongodb(&self) -> Result<()> {
        info!("Installing MongoDB");
        self.ssh_exec(r#"
            sudo apt-get update
            sudo apt-get install -y mongodb
            sudo systemctl enable mongodb
            sudo systemctl start mongodb
        "#)?;
        Ok(())
    }

    fn run_custom_provision_scripts(&self, config: &VmConfig) -> Result<()> {
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
mod provisioner;
use provisioner::TartProvisioner;

impl Provider for TartProvider {
    fn provision(&self, container: Option<&str>) -> Result<()> {
        let instance_name = self.resolve_instance_name(container)?;

        let provisioner = TartProvisioner::new(
            instance_name.clone(),
            self.get_sync_directory(),
        );

        provisioner.provision(&self.config)?;

        vm_println!("{}", MESSAGES.provision_success);
        Ok(())
    }
}
```

---

### 3. Enhanced Status Reports (Batched SSH)

**New File**: `rust/vm-provider/src/tart/scripts/collect_metrics.sh`

```bash
#!/bin/bash
# Collect all metrics in one SSH call and emit JSON

cpu_percent=$(top -bn1 | grep "Cpu(s)" | awk '{print $2}' | sed 's/%us,//')
memory_used_kb=$(free | grep Mem | awk '{print $3}')
memory_total_kb=$(free | grep Mem | awk '{print $2}')
disk_info=$(df -BG / | tail -1)
disk_used_gb=$(echo $disk_info | awk '{print $3}' | sed 's/G//')
disk_total_gb=$(echo $disk_info | awk '{print $2}' | sed 's/G//')
uptime_str=$(uptime -p)

# Convert memory to MB
memory_used_mb=$((memory_used_kb / 1024))
memory_total_mb=$((memory_total_kb / 1024))

# Check systemd services
postgres_status="false"
redis_status="false"
mongodb_status="false"

if systemctl is-active --quiet postgresql 2>/dev/null; then
    postgres_status="true"
fi

if systemctl is-active --quiet redis-server 2>/dev/null; then
    redis_status="true"
fi

if systemctl is-active --quiet mongodb 2>/dev/null; then
    mongodb_status="true"
fi

# Emit JSON
cat <<EOF
{
  "cpu_percent": ${cpu_percent:-0},
  "memory_used_mb": ${memory_used_mb:-0},
  "memory_limit_mb": ${memory_total_mb:-0},
  "disk_used_gb": ${disk_used_gb:-0},
  "disk_total_gb": ${disk_total_gb:-0},
  "uptime": "${uptime_str}",
  "services": [
    {"name": "postgresql", "is_running": ${postgres_status}},
    {"name": "redis", "is_running": ${redis_status}},
    {"name": "mongodb", "is_running": ${mongodb_status}}
  ]
}
EOF
```

**Update provider.rs**:
```rust
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
    let metrics_script = include_str!("scripts/collect_metrics.sh");
    let output = cmd!("tart", "ssh", instance, "--", "sh", "-c", metrics_script)
        .stderr_capture()
        .read()
        .map_err(|e| VmError::Provider(format!("SSH command failed: {}", e)))?;

    parse_metrics_json(&output)
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

---

### 4. ProviderContext Support

**Update provider.rs**:
```rust
fn create_with_context(&self, context: &ProviderContext) -> Result<()> {
    let effective_config = context.global_config.as_ref().unwrap_or(&self.config);
    self.apply_config_from_context(effective_config)?;
    self.create_internal()
}

fn create_instance_with_context(&self, instance_name: &str, context: &ProviderContext) -> Result<()> {
    let effective_config = context.global_config.as_ref().unwrap_or(&self.config);
    self.apply_config_from_context(effective_config)?;
    self.create_instance_internal(instance_name)
}

fn start_with_context(&self, container: Option<&str>, context: &ProviderContext) -> Result<()> {
    let instance_name = self.resolve_instance_name(container)?;

    if let Some(global_config) = &context.global_config {
        info!("Applying config updates to Tart VM");
        self.apply_runtime_config(&instance_name, global_config)?;
    }

    self.start(Some(&instance_name))
}

fn restart_with_context(&self, container: Option<&str>, context: &ProviderContext) -> Result<()> {
    let instance_name = self.resolve_instance_name(container)?;

    if let Some(global_config) = &context.global_config {
        info!("Applying config updates to Tart VM");
        self.apply_runtime_config(&instance_name, global_config)?;
    }

    self.restart(Some(&instance_name))
}

fn apply_runtime_config(&self, instance: &str, config: &VmConfig) -> Result<()> {
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

---

### 5. TempProvider Support

**Update provider.rs**:
```rust
impl TempProvider for TartProvider {
    fn update_mounts(&self, state: &TempVmState) -> Result<()> {
        info!("Updating mounts for Tart VM: {}", state.name);
        self.stop(Some(&state.name))?;
        self.recreate_with_mounts(state)?;
        Ok(())
    }

    fn recreate_with_mounts(&self, state: &TempVmState) -> Result<()> {
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

        self.start(Some(&state.name))?;
        Ok(())
    }

    fn check_container_health(&self, container_name: &str) -> Result<bool> {
        if !self.is_instance_running(container_name)? {
            return Ok(false);
        }

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
    fn as_temp_provider(&self) -> Option<&dyn TempProvider> {
        Some(self)  // Changed from None
    }
}
```

---

### 6. Force Kill Implementation

**Update provider.rs**:
```rust
fn kill(&self, container: Option<&str>) -> Result<()> {
    let instance_name = self.resolve_instance_name(container)?;

    warn!("Force killing Tart VM: {}", instance_name);

    // Try graceful stop first
    if let Err(e) = self.stop(Some(&instance_name)) {
        warn!("Graceful stop failed: {}", e);
    } else {
        info!("VM stopped gracefully");
        return Ok(());
    }

    // Force stop using Tart CLI
    cmd!("tart", "stop", "--force", &instance_name)
        .run()
        .map_err(|e| VmError::Provider(format!("Failed to force stop VM: {}", e)))?;

    info!("Tart VM force-stopped via CLI");
    Ok(())
}
```

---

## Implementation Summary

### Code Changes

**New Files**:
- `rust/vm-provider/src/tart/provisioner.rs` (~280 LOC)
- `rust/vm-provider/src/tart/scripts/collect_metrics.sh` (~50 LOC)

**Modified Files**:
- `rust/vm-provider/src/tart/provider.rs` (~480 LOC added)

**Total**: ~810 LOC

### Testing Strategy

**Unit Tests**:
- SSH path handling correctness
- Provisioning framework detection
- Metrics parsing
- Context config application
- TempProvider mount updates
- Force kill fallback logic

**Integration Tests** (macOS only):
- Real Tart VM provisioning for Node.js, Python, Ruby
- Status report metrics collection
- Config updates without destroy
- Temporary VM lifecycle
- Force kill on hung VMs

**Platform Requirements**:
- macOS 13+ (Ventura or later)
- Apple Silicon (M1/M2/M3) or Intel
- Tart CLI installed (`brew install cirruslabs/cli/tart`)

---

## Risks and Mitigations

### Risk 1: Provisioning Complexity
**Risk**: Framework detection and package installation may fail
**Mitigation**: Start with well-tested frameworks (Node.js, Python), add error handling and fallbacks

### Risk 2: SSH Performance
**Risk**: Multiple SSH calls could slow down operations
**Mitigation**: Batch all metrics collection into single SSH round-trip using embedded script

### Risk 3: Mount Updates Require Restart
**Risk**: TempProvider mount updates cause VM downtime
**Mitigation**: Document as expected behavior, ensure restart is fast

### Risk 4: macOS-Only Testing
**Risk**: Code can't be tested in CI/CD on Linux
**Mitigation**: Use conditional compilation, skip tests on non-macOS platforms

---

## Success Metrics

### Quantitative Goals
- ✅ Tart passes 100% of Docker provider test suite (macOS only)
- ✅ Provisioning installs frameworks successfully (Node.js, Python, Ruby, Rust, Go)
- ✅ SSH lands in correct directory 100% of the time
- ✅ Enhanced status reports return data within 3 seconds
- ✅ Config updates work without destroy/recreate
- ✅ `vm temp` workflow functional

### Qualitative Goals
- ✅ macOS developers can use Tart as primary provider
- ✅ No feature limitations compared to Docker
- ✅ Tart provider rated ⭐⭐⭐⭐⭐ (same as Docker)

---

## Estimated Effort

**Total Implementation**: ~810 LOC
**Estimated Time**: 24-32 hours (3-4 days)

**Breakdown**:
- Provisioning system: 10-12 hours
- Enhanced status: 4-5 hours
- ProviderContext: 3-4 hours
- TempProvider: 5-6 hours
- SSH path fix: 1-2 hours
- Force kill: 2-3 hours

---

## Feature Comparison (Before vs After)

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

## Conclusion

This proposal provides a **single comprehensive PR** to bring Tart from basic support to full Docker parity. With **~810 LOC and 24-32 hours of work**, the Tart provider will:

1. ✅ **Fix critical bugs** (SSH path handling)
2. ✅ **Add essential features** (provisioning)
3. ✅ **Enable advanced capabilities** (status, context, temp VMs)
4. ✅ **Improve reliability** (force kill)

**Impact**: macOS developers (especially on Apple Silicon) will have a **native, performant VM provider** with zero feature limitations compared to Docker. This enables:
- Faster VM performance via Apple Virtualization Framework
- Better battery life (native vs. Docker Desktop)
- Seamless integration with macOS ecosystem

**Recommendation**: Approve and implement as a single PR for maximum efficiency and atomic feature delivery.
