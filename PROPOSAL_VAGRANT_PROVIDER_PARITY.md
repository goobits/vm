# Proposal: Vagrant Provider Feature Parity with Docker

## Status
**Proposed** - Not yet implemented

## Executive Summary

Bring the Vagrant provider to feature parity with Docker by implementing 4 missing capabilities in a single comprehensive PR: enhanced status reporting, TempProvider support, full ProviderContext support with Vagrantfile regeneration, and a VirtualBox-first force-kill path. This will elevate Vagrant from **40% advanced feature coverage to 100%**, making it a first-class alternative to Docker for teams using VirtualBox, VMware, or Hyper-V.

**Current State**: ⭐⭐⭐⭐ (95% core, 40% advanced)
**Target State**: ⭐⭐⭐⭐⭐ (100% core, 100% advanced) - Full Docker parity

---

## Problem Statement

The Vagrant provider currently has **4 critical gaps** compared to Docker:

### Gap Analysis

| Feature | Docker | Vagrant | Impact |
|---------|--------|---------|--------|
| Enhanced Status Reports | ✅ Full | ❌ Returns error | Users can't see CPU/memory/disk usage or service health |
| TempProvider Support | ✅ Full | ❌ Not supported | `vm temp` workflow completely unavailable |
| ProviderContext Support | ✅ Regenerates config | ⚠️ Passes via env var | Config changes require manual `vm destroy && vm create` |
| Force Kill | ✅ Distinct operation | ⚠️ Calls `destroy()` | Can't force-kill hung VMs without destroying them |

### User Impact

**Without these features, Vagrant users cannot:**
1. Monitor VM resource usage in real-time (no dashboard metrics)
2. Use temporary VMs for quick testing (`vm temp` command fails)
3. Apply global config changes without destroying/recreating VMs
4. Force-kill stuck VMs without losing all data

**Business Case**: Teams using VirtualBox/VMware (common in enterprise environments with Docker restrictions) are forced to use a second-class provider experience.

---

## Proposed Solution

Implement all 4 features in a **single comprehensive PR** (~490 LOC) to achieve full Docker parity.

### Implementation Overview

The PR will add:
- **Batched SSH metrics collection** for status reports
- **Vagrantfile regeneration** for ProviderContext support
- **TempProvider trait implementation** for temporary VM support
- **VirtualBox-first force-kill** with warnings for other providers

---

## Detailed Implementation

### 1. Enhanced Status Reports (Batched SSH)

**Goal**: Implement `get_status_report()` to return real-time VM metrics in a single SSH round-trip

**New File**: `rust/vm-provider/src/vagrant/scripts/collect_metrics.sh`

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
// rust/vm-provider/src/vagrant/provider.rs

fn get_status_report(&self, container: Option<&str>) -> Result<VmStatusReport> {
    let instance_name = self.resolve_instance_name(container)?;

    let is_running = self.is_instance_running(&instance_name)?;

    if !is_running {
        return Ok(VmStatusReport {
            name: instance_name.clone(),
            provider: "vagrant".to_string(),
            is_running: false,
            ..Default::default()
        });
    }

    let metrics = self.collect_metrics(&instance_name)?;

    Ok(VmStatusReport {
        name: instance_name,
        provider: "vagrant".to_string(),
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
    let instance_dir = self.get_instance_dir(instance)?;

    let metrics_script = include_str!("scripts/collect_metrics.sh");
    let output = cmd!("vagrant", "ssh", "-c", metrics_script)
        .dir(&instance_dir)
        .stderr_null()
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

### 2. Full ProviderContext Support with Vagrantfile Regeneration

**Goal**: Make `start_with_context()` and `restart_with_context()` regenerate Vagrantfile

**Update provider.rs**:
```rust
fn start_with_context(&self, container: Option<&str>, context: &ProviderContext) -> Result<()> {
    let instance_name = self.resolve_instance_name(container)?;

    if let Some(global_config) = &context.global_config {
        info!("Regenerating Vagrantfile with updated global config");
        self.regenerate_vagrantfile(&instance_name, global_config)?;
    }

    self.start(Some(&instance_name))
}

fn restart_with_context(&self, container: Option<&str>, context: &ProviderContext) -> Result<()> {
    let instance_name = self.resolve_instance_name(container)?;

    if let Some(global_config) = &context.global_config {
        info!("Regenerating Vagrantfile with updated global config");
        self.regenerate_vagrantfile(&instance_name, global_config)?;
    }

    self.restart(Some(&instance_name))
}

fn regenerate_vagrantfile(&self, instance: &str, global_config: &GlobalConfig) -> Result<()> {
    let instance_dir = self.get_instance_dir(instance)?;
    let generated_path = instance_dir.join("Vagrantfile.vmtool");

    let vagrantfile_content = self.generate_vagrantfile_content(global_config)?;

    std::fs::write(&generated_path, vagrantfile_content)
        .map_err(|e| VmError::Provider(format!("Failed to write generated Vagrantfile: {}", e)))?;

    info!("Wrote regenerated Vagrantfile to {:?}", generated_path);
    Ok(())
}

fn generate_vagrantfile_content(&self, global_config: &GlobalConfig) -> Result<String> {
    // Reuse existing Vagrantfile generation logic from create()
    // This method would extract the template rendering logic
    // and accept GlobalConfig as parameter

    let template = self.vagrantfile_template();
    let rendered = self.render_template(&template, global_config)?;
    Ok(rendered)
}
```

Every lifecycle command exports `VAGRANT_VAGRANTFILE=Vagrantfile.vmtool`, so the generated file is used while any user-maintained `Vagrantfile` stays untouched.

---

### 3. TempProvider Support

**Goal**: Implement `TempProvider` trait for `vm temp` workflow

**Update provider.rs**:
```rust
impl TempProvider for VagrantProvider {
    fn update_mounts(&self, state: &TempVmState) -> Result<()> {
        let instance_dir = self.get_instance_dir(&state.name)?;
        let vagrantfile_path = instance_dir.join("Vagrantfile.vmtool");

        let current_content = std::fs::read_to_string(&vagrantfile_path)
            .map_err(|e| VmError::Provider(format!("Failed to read Vagrantfile: {}", e)))?;

        let new_content = self.update_synced_folders(current_content, &state.mounts)?;

        std::fs::write(&vagrantfile_path, new_content)
            .map_err(|e| VmError::Provider(format!("Failed to write Vagrantfile: {}", e)))?;

        self.recreate_with_mounts(state)
    }

    fn recreate_with_mounts(&self, state: &TempVmState) -> Result<()> {
        use duct::cmd;

        let instance_dir = self.get_instance_dir(&state.name)?;

        info!("Reloading Vagrant VM to apply mount changes");

        cmd!("vagrant", "reload")
            .dir(&instance_dir)
            .run()
            .map_err(|e| VmError::Provider(format!("Failed to reload VM: {}", e)))?;

        Ok(())
    }

    fn check_container_health(&self, container_name: &str) -> Result<bool> {
        if !self.is_instance_running(container_name)? {
            return Ok(false);
        }

        let ssh_test = self.ssh_exec_capture(container_name, "echo healthy");
        Ok(ssh_test.is_ok())
    }

    fn is_container_running(&self, container_name: &str) -> Result<bool> {
        self.is_instance_running(container_name)
    }
}

impl VagrantProvider {
    fn update_synced_folders(&self, vagrantfile: String, mounts: &[Mount]) -> Result<String> {
        let mut lines: Vec<String> = vagrantfile.lines().map(|s| s.to_string()).collect();

        let start_marker = "# BEGIN SYNCED FOLDERS";
        let end_marker = "# END SYNCED FOLDERS";

        // Remove old synced folder section
        if let (Some(start_idx), Some(end_idx)) = (
            lines.iter().position(|l| l.contains(start_marker)),
            lines.iter().position(|l| l.contains(end_marker))
        ) {
            lines.drain(start_idx..=end_idx);
        }

        // Insert new synced folder section
        let mut folder_lines = vec!["  # BEGIN SYNCED FOLDERS".to_string()];

        for mount in mounts {
            let mount_type = match mount.permission {
                MountPermission::ReadWrite => "",
                MountPermission::ReadOnly => ", mount_options: [\"ro\"]",
            };

            folder_lines.push(format!(
                "  config.vm.synced_folder \"{}\", \"{}\"{}",
                mount.host_path.display(),
                mount.guest_path.display(),
                mount_type
            ));
        }

        folder_lines.push("  # END SYNCED FOLDERS".to_string());

        // Insert after the config.vm.box line
        if let Some(box_idx) = lines.iter().position(|l| l.contains("config.vm.box")) {
            lines.splice(box_idx + 1..box_idx + 1, folder_lines);
        } else {
            return Err(VmError::Provider("Could not find config.vm.box in Vagrantfile".to_string()));
        }

        Ok(lines.join("\n"))
    }
}

impl Provider for VagrantProvider {
    fn as_temp_provider(&self) -> Option<&dyn TempProvider> {
        Some(self)  // Changed from None
    }
}
```

---

### 4. Force Kill (VirtualBox First)

**Goal**: Implement distinct `kill()` that force-kills VM processes

**Update provider.rs**:
```rust
fn kill(&self, container: Option<&str>) -> Result<()> {
    use duct::cmd;

    let instance_name = self.resolve_instance_name(container)?;
    let instance_dir = self.get_instance_dir(&instance_name)?;

    warn!("Force killing Vagrant VM: {}", instance_name);

    // Try graceful halt first
    let halt_result = cmd!("vagrant", "halt", "--force")
        .dir(&instance_dir)
        .run();

    if halt_result.is_ok() {
        info!("VM halted gracefully");
        return Ok(());
    }

    // If graceful halt fails, attempt provider-specific hard stop
    warn!("Graceful halt failed, attempting provider-specific hard stop");

    let vm_id_output = cmd!("vagrant", "global-status", "--prune")
        .dir(&instance_dir)
        .read()
        .map_err(|e| VmError::Provider(format!("Failed to get VM ID: {}", e)))?;

    let vm_id = self.parse_vm_id(&vm_id_output, &instance_name)?;

    match self.detect_vagrant_provider()? {
        "virtualbox" => {
            cmd!("VBoxManage", "controlvm", &vm_id, "poweroff")
                .run()
                .map_err(|e| VmError::Provider(format!("Failed to kill VirtualBox VM: {}", e)))?;
            info!("VirtualBox VM powered off");
            Ok(())
        }
        provider => {
            warn!(
                "Force kill not yet implemented for provider: {}. Fallback to vm destroy",
                provider
            );
            Err(VmError::Provider(format!(
                "Force kill not available for {}; use `vm destroy --force`",
                provider
            )))
        }
    }
}

fn detect_vagrant_provider(&self) -> Result<String> {
    // Read from .vagrant directory or Vagrantfile
    // Default to virtualbox if not found
    Ok("virtualbox".to_string())
}

fn parse_vm_id(&self, global_status_output: &str, instance_name: &str) -> Result<String> {
    for line in global_status_output.lines().skip(2) {
        if line.contains(instance_name) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if !parts.is_empty() {
                return Ok(parts[0].to_string());
            }
        }
    }

    Err(VmError::Provider(format!(
        "Could not find VM ID for instance: {}",
        instance_name
    )))
}
```

---

## Implementation Summary

### Code Changes

**New Files**:
- `rust/vm-provider/src/vagrant/scripts/collect_metrics.sh` (~50 LOC)

**Modified Files**:
- `rust/vm-provider/src/vagrant/provider.rs` (~440 LOC added)

**Total**: ~490 LOC

### Testing Strategy

**Unit Tests**:
- Status report metrics parsing
- Vagrantfile regeneration logic
- Synced folder updates
- VM ID parsing
- Provider detection
- Force kill fallback logic

**Integration Tests**:
- Real Vagrant VM status collection (VirtualBox)
- Vagrantfile regeneration and reload
- Temporary VM lifecycle with mount updates
- Force kill on hung VirtualBox VMs
- Warnings on unsupported providers

**Platform Requirements**:
- VirtualBox 6.0+ (primary testing)
- VMware Desktop (manual testing)
- Hyper-V (manual testing)
- Vagrant 2.2+

---

## Risks and Mitigations

### Risk 1: Vagrantfile Parsing Complexity
**Risk**: Updating Vagrantfile synced folders requires parsing Ruby code
**Mitigation**: Use marker comments (`# BEGIN/END SYNCED FOLDERS`) to isolate sections. Generate full Vagrantfile from scratch if needed, similar to Docker's docker-compose.yml approach

### Risk 2: Platform-Specific Kill Commands
**Risk**: Force kill requires different commands for VirtualBox/VMware/Hyper-V
**Mitigation**: Ship VirtualBox support first, emit clear warning for other providers, plan follow-up support for VMware/Hyper-V

### Risk 3: SSH Performance for Status Reports
**Risk**: Status reports require SSH commands, may be slow
**Mitigation**: Batch all commands into single SSH session with JSON output, complete in < 2 seconds

### Risk 4: Vagrant Reload Downtime
**Risk**: `vagrant reload` causes brief downtime for mount updates
**Mitigation**: Document as expected behavior; this is a Vagrant limitation, not a VM tool limitation

---

## Success Metrics

### Quantitative Goals
- ✅ Vagrant passes 100% of Docker provider test suite
- ✅ Enhanced status reports return data within 2 seconds (single SSH round-trip)
- ✅ Config updates via context work without `vm destroy`
- ✅ Temp VM workflows functional (document first start timing expectations)
- ✅ Force kill terminates VirtualBox VMs within 5 seconds
- ✅ VMware/Hyper-V emit clear warnings about unsupported force kill

### Qualitative Goals
- ✅ Users can choose Vagrant or Docker based on infrastructure needs, not feature limitations
- ✅ Enterprise teams with Docker restrictions have full-featured alternative
- ✅ Vagrant provider rated ⭐⭐⭐⭐⭐ (same as Docker)

---

## Estimated Effort

**Total Implementation**: ~490 LOC
**Estimated Time**: 15-21 hours (2-3 days)

**Breakdown**:
- Enhanced status reports: 4-6 hours
- ProviderContext support: 3-4 hours
- TempProvider support: 6-8 hours
- Force kill: 2-3 hours

---

## Feature Comparison (Before vs After)

| Feature | Before | After | Docker Parity |
|---------|--------|-------|---------------|
| **Enhanced Status Reports** | ❌ Returns error | ✅ CPU/Memory/Disk/Services | ✅ 100% |
| **TempProvider Support** | ❌ Not implemented | ✅ Full support | ✅ 100% |
| **ProviderContext Support** | ⚠️ Passes via env var | ✅ Regenerates Vagrantfile | ✅ 100% |
| **Force Kill** | ⚠️ Calls destroy() | ✅ VirtualBox hard stop | ✅ 100% |
| **Multi-Instance** | ✅ Works | ✅ Works | ✅ 100% |
| **Provisioning** | ✅ Works | ✅ Works | ✅ 100% |
| **SSH** | ✅ Works | ✅ Works | ✅ 100% |
| **Logs** | ✅ Works | ✅ Works | ✅ 100% |
| **Overall Rating** | ⭐⭐⭐⭐ (40% advanced) | ⭐⭐⭐⭐⭐ (100% advanced) | ✅ Full Parity |

---

## Future Enhancements (Out of Scope)

These are **not** required for parity but could be added later:

1. **Parallel SSH Commands**: Use GNU parallel for faster status collection
2. **Vagrantfile Templates**: Use Tera templates like Docker (more maintainable)
3. **Provider Auto-Detection**: Detect VirtualBox vs VMware automatically
4. **Metrics Caching**: Cache resource metrics to reduce SSH overhead
5. **Health Check Endpoints**: Poll HTTP endpoints for service health instead of systemctl
6. **VMware/Hyper-V Force Kill**: Extend force kill support to other providers

---

## Conclusion

This proposal provides a **single comprehensive PR** to bring Vagrant to full Docker parity. With **~490 LOC and 15-21 hours of work**, the Vagrant provider will support all advanced features:

1. ✅ Enhanced status reports with real-time metrics
2. ✅ Full ProviderContext support with config regeneration
3. ✅ TempProvider for `vm temp` workflows
4. ✅ Proper force-kill for VirtualBox with clear warnings for others

**Impact**: Enterprise teams using VirtualBox, VMware, or Hyper-V will have a **first-class VM provider experience** with no feature limitations compared to Docker.

**Recommendation**: Approve and implement as a single PR for maximum efficiency and atomic feature delivery.
