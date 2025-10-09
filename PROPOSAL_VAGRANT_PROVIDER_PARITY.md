# Proposal: Vagrant Provider Feature Parity with Docker

## Status
**Proposed** - Not yet implemented

## Executive Summary

Bring the Vagrant provider to feature parity with Docker by implementing 4 missing capabilities: enhanced status reporting, TempProvider support, full ProviderContext support with Vagrantfile regeneration, and proper force-kill functionality. This will elevate Vagrant from **40% advanced feature coverage to 100%**, making it a first-class alternative to Docker for teams using VirtualBox, VMware, or Hyper-V.

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

Implement 4 features across 4 incremental PRs to achieve full Docker parity.

### PR #1: Enhanced Status Reports (~120 LOC)

**Goal**: Implement `get_status_report()` to return real-time VM metrics

**Implementation**:
```rust
// rust/vm-provider/src/vagrant/provider.rs

fn get_status_report(&self, container: Option<&str>) -> Result<VmStatusReport> {
    let instance_name = self.resolve_instance_name(container)?;

    // 1. Get VM running state via `vagrant status`
    let is_running = self.is_instance_running(&instance_name)?;

    if !is_running {
        return Ok(VmStatusReport {
            name: instance_name.clone(),
            provider: "vagrant".to_string(),
            is_running: false,
            ..Default::default()
        });
    }

    // 2. Get resource usage via SSH commands
    let resources = self.get_resource_usage(&instance_name)?;

    // 3. Get service status for known services
    let services = self.get_service_statuses(&instance_name)?;

    // 4. Get uptime
    let uptime = self.get_uptime(&instance_name)?;

    Ok(VmStatusReport {
        name: instance_name,
        provider: "vagrant".to_string(),
        container_id: None, // Vagrant doesn't have container IDs
        is_running: true,
        uptime: Some(uptime),
        resources,
        services,
    })
}

fn get_resource_usage(&self, instance: &str) -> Result<ResourceUsage> {
    // SSH into VM and run system commands
    let cpu_cmd = "top -bn1 | grep 'Cpu(s)' | awk '{print $2}' | cut -d'%' -f1";
    let mem_cmd = "free -m | awk 'NR==2{printf \"%s %s\", $3,$2}'";
    let disk_cmd = "df -BG / | awk 'NR==2{printf \"%s %s\", $3,$2}'";

    let cpu_percent = self.ssh_exec_capture(instance, cpu_cmd)
        .and_then(|s| s.trim().parse::<f64>().ok());

    let mem_info = self.ssh_exec_capture(instance, mem_cmd).ok();
    let (memory_used_mb, memory_limit_mb) = mem_info
        .and_then(|s| {
            let parts: Vec<&str> = s.split_whitespace().collect();
            if parts.len() == 2 {
                let used = parts[0].parse::<u64>().ok()?;
                let total = parts[1].parse::<u64>().ok()?;
                Some((used, total))
            } else {
                None
            }
        })
        .unwrap_or((None, None));

    let disk_info = self.ssh_exec_capture(instance, disk_cmd).ok();
    let (disk_used_gb, disk_total_gb) = disk_info
        .and_then(|s| {
            let parts: Vec<&str> = s.split_whitespace().collect();
            if parts.len() == 2 {
                // Remove 'G' suffix and parse
                let used = parts[0].trim_end_matches('G').parse::<f64>().ok()?;
                let total = parts[1].trim_end_matches('G').parse::<f64>().ok()?;
                Some((used, total))
            } else {
                None
            }
        })
        .unzip();

    Ok(ResourceUsage {
        cpu_percent,
        memory_used_mb,
        memory_limit_mb,
        disk_used_gb,
        disk_total_gb,
    })
}

fn get_service_statuses(&self, instance: &str) -> Result<Vec<ServiceStatus>> {
    let mut services = Vec::new();

    // Check for common services (PostgreSQL, Redis, MongoDB)
    for (service_name, systemd_unit) in &[
        ("PostgreSQL", "postgresql"),
        ("Redis", "redis"),
        ("MongoDB", "mongod"),
    ] {
        let is_running = self.ssh_exec_capture(
            instance,
            &format!("systemctl is-active {} 2>/dev/null || echo inactive", systemd_unit)
        )
        .map(|s| s.trim() == "active")
        .unwrap_or(false);

        services.push(ServiceStatus {
            name: service_name.to_string(),
            is_running,
            port: None, // Could extract from config
            host_port: None,
            metrics: None,
            error: None,
        });
    }

    Ok(services)
}

fn get_uptime(&self, instance: &str) -> Result<String> {
    self.ssh_exec_capture(instance, "uptime -p")
        .map(|s| s.trim().to_string())
}

// Helper: Execute SSH command and capture output
fn ssh_exec_capture(&self, instance: &str, cmd: &str) -> Result<String> {
    use duct::cmd;

    let instance_dir = self.get_instance_dir(instance)?;
    let output = cmd!("vagrant", "ssh", "-c", cmd)
        .dir(&instance_dir)
        .stderr_null()
        .read()
        .map_err(|e| VmError::Provider(format!("SSH command failed: {}", e)))?;

    Ok(output)
}
```

**Benefits**:
- Users can see real-time VM metrics with `vm status`
- Enhanced dashboard displays resource usage
- Service health monitoring

**Estimated Effort**: 4-6 hours

---

### PR #2: Full ProviderContext Support with Vagrantfile Regeneration (~80 LOC)

**Goal**: Make `start_with_context()` and `restart_with_context()` regenerate Vagrantfile

**Current Behavior**:
```rust
// Currently just falls back to default implementation (ignores context)
fn start_with_context(&self, container: Option<&str>, context: &ProviderContext) -> Result<()> {
    let _ = context;  // IGNORED!
    self.start(container)
}
```

**New Implementation**:
```rust
// rust/vm-provider/src/vagrant/provider.rs

fn start_with_context(&self, container: Option<&str>, context: &ProviderContext) -> Result<()> {
    let instance_name = self.resolve_instance_name(container)?;

    // Regenerate Vagrantfile if context has config
    if let Some(global_config) = &context.global_config {
        info!("Regenerating Vagrantfile with updated global config");
        self.regenerate_vagrantfile(&instance_name, global_config)?;
    }

    // Now start with updated config
    self.start(Some(&instance_name))
}

fn restart_with_context(&self, container: Option<&str>, context: &ProviderContext) -> Result<()> {
    let instance_name = self.resolve_instance_name(container)?;

    // Regenerate Vagrantfile if context has config
    if let Some(global_config) = &context.global_config {
        info!("Regenerating Vagrantfile with updated global config");
        self.regenerate_vagrantfile(&instance_name, global_config)?;
    }

    // Now restart with updated config
    // Use `vagrant reload` to apply config changes
    self.restart(Some(&instance_name))
}

fn regenerate_vagrantfile(&self, instance: &str, global_config: &GlobalConfig) -> Result<()> {
    let instance_dir = self.get_instance_dir(instance)?;
    let vagrantfile_path = instance_dir.join("Vagrantfile");

    // Generate new Vagrantfile content
    let vagrantfile_content = self.generate_vagrantfile_content(global_config)?;

    // Write new Vagrantfile
    std::fs::write(&vagrantfile_path, vagrantfile_content)
        .map_err(|e| VmError::Provider(format!("Failed to write Vagrantfile: {}", e)))?;

    info!("Regenerated Vagrantfile at {:?}", vagrantfile_path);
    Ok(())
}

fn generate_vagrantfile_content(&self, global_config: &GlobalConfig) -> Result<String> {
    // Reuse existing Vagrantfile generation logic from create()
    // Extract current generate_vagrantfile() into a shared method
    // that accepts GlobalConfig as parameter

    // Implementation would be similar to Docker's template regeneration
    // but for Vagrantfile format instead of docker-compose.yml

    todo!("Extract and reuse Vagrantfile generation logic")
}
```

**Benefits**:
- Users can update global config and apply with `vm restart` (no destroy needed)
- Config changes propagate without data loss
- Matches Docker behavior exactly

**Estimated Effort**: 3-4 hours

---

### PR #3: TempProvider Support (~200 LOC)

**Goal**: Implement `TempProvider` trait for `vm temp` workflow

**Implementation**:
```rust
// rust/vm-provider/src/vagrant/provider.rs

impl TempProvider for VagrantProvider {
    fn update_mounts(&self, state: &TempVmState) -> Result<()> {
        // For Vagrant, we need to update Vagrantfile with new synced folders
        // then reload the VM

        let instance_dir = self.get_instance_dir(&state.name)?;
        let vagrantfile_path = instance_dir.join("Vagrantfile");

        // Read current Vagrantfile
        let current_content = std::fs::read_to_string(&vagrantfile_path)
            .map_err(|e| VmError::Provider(format!("Failed to read Vagrantfile: {}", e)))?;

        // Update synced_folder entries
        let new_content = self.update_synced_folders(current_content, &state.mounts)?;

        // Write updated Vagrantfile
        std::fs::write(&vagrantfile_path, new_content)
            .map_err(|e| VmError::Provider(format!("Failed to write Vagrantfile: {}", e)))?;

        // Reload VM to apply changes
        self.recreate_with_mounts(state)
    }

    fn recreate_with_mounts(&self, state: &TempVmState) -> Result<()> {
        use duct::cmd;

        let instance_dir = self.get_instance_dir(&state.name)?;

        // Use `vagrant reload` to apply mount changes
        // This is safer than destroy/recreate for temp VMs
        info!("Reloading Vagrant VM to apply mount changes");

        cmd!("vagrant", "reload")
            .dir(&instance_dir)
            .run()
            .map_err(|e| VmError::Provider(format!("Failed to reload VM: {}", e)))?;

        Ok(())
    }

    fn check_container_health(&self, container_name: &str) -> Result<bool> {
        // For Vagrant, health = VM is running + SSH is responsive

        if !self.is_instance_running(container_name)? {
            return Ok(false);
        }

        // Test SSH connectivity
        let ssh_test = self.ssh_exec_capture(container_name, "echo healthy");
        Ok(ssh_test.is_ok())
    }

    fn is_container_running(&self, container_name: &str) -> Result<bool> {
        self.is_instance_running(container_name)
    }
}

impl VagrantProvider {
    fn update_synced_folders(&self, vagrantfile: String, mounts: &[Mount]) -> Result<String> {
        // Parse Vagrantfile and update config.vm.synced_folder entries
        // This is a simplified approach - could use regex or proper Ruby parsing

        let mut lines: Vec<String> = vagrantfile.lines().map(|s| s.to_string()).collect();

        // Find the synced_folder section and replace it
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
        let mut folder_lines = vec![
            "  # BEGIN SYNCED FOLDERS".to_string(),
        ];

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

// Update VagrantProvider to return TempProvider trait object
impl Provider for VagrantProvider {
    // ... existing methods ...

    fn as_temp_provider(&self) -> Option<&dyn TempProvider> {
        Some(self)  // Changed from None
    }
}
```

**Benefits**:
- `vm temp` command works with Vagrant
- Users can create quick throwaway VMs
- Full feature parity with Docker for temp workflows

**Estimated Effort**: 6-8 hours

---

### PR #4: Proper Force Kill Implementation (~40 LOC)

**Goal**: Implement distinct `kill()` that force-kills VM processes

**Current Behavior**:
```rust
fn kill(&self, container: Option<&str>) -> Result<()> {
    // Currently just delegates to destroy
    self.destroy(container)
}
```

**New Implementation**:
```rust
fn kill(&self, container: Option<&str>) -> Result<()> {
    use duct::cmd;

    let instance_name = self.resolve_instance_name(container)?;
    let instance_dir = self.get_instance_dir(&instance_name)?;

    warn!("Force killing Vagrant VM: {}", instance_name);

    // Try graceful halt first (with short timeout)
    let halt_result = cmd!("vagrant", "halt", "--force")
        .dir(&instance_dir)
        .run();

    if halt_result.is_ok() {
        info!("VM halted gracefully");
        return Ok(());
    }

    // If graceful halt fails, kill VirtualBox/VMware processes directly
    warn!("Graceful halt failed, killing VM processes forcefully");

    // Get VM ID from Vagrant
    let vm_id_output = cmd!("vagrant", "global-status", "--prune")
        .dir(&instance_dir)
        .read()
        .map_err(|e| VmError::Provider(format!("Failed to get VM ID: {}", e)))?;

    // Parse VM ID from output
    let vm_id = self.parse_vm_id(&vm_id_output, &instance_name)?;

    // Force kill based on provider type
    match self.detect_vagrant_provider()? {
        "virtualbox" => {
            cmd!("VBoxManage", "controlvm", &vm_id, "poweroff")
                .run()
                .map_err(|e| VmError::Provider(format!("Failed to kill VirtualBox VM: {}", e)))?;
        }
        "vmware_desktop" | "vmware_fusion" => {
            // VMware force kill
            cmd!("vmrun", "stop", &vm_id, "hard")
                .run()
                .map_err(|e| VmError::Provider(format!("Failed to kill VMware VM: {}", e)))?;
        }
        "hyperv" => {
            // Hyper-V force kill
            cmd!("powershell", "-Command", &format!("Stop-VM -Name '{}' -Force", vm_id))
                .run()
                .map_err(|e| VmError::Provider(format!("Failed to kill Hyper-V VM: {}", e)))?;
        }
        provider => {
            return Err(VmError::Provider(format!(
                "Force kill not implemented for provider: {}",
                provider
            )));
        }
    }

    info!("VM processes killed forcefully");
    Ok(())
}

fn detect_vagrant_provider(&self) -> Result<String> {
    // Read from .vagrant directory or Vagrantfile
    // Default to virtualbox if not found
    Ok("virtualbox".to_string())
}

fn parse_vm_id(&self, global_status_output: &str, instance_name: &str) -> Result<String> {
    // Parse vagrant global-status output to find VM ID
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

**Benefits**:
- Can force-kill hung VMs without destroying data
- Matches Docker's force-kill behavior
- Supports VirtualBox, VMware, and Hyper-V

**Estimated Effort**: 2-3 hours

---

## Implementation Plan

### Incremental Rollout (4 PRs)

| PR | Feature | LOC | Effort | Risk | Dependencies |
|----|---------|-----|--------|------|--------------|
| 1 | Enhanced Status Reports | ~120 | 4-6h | Low | None |
| 2 | ProviderContext Support | ~80 | 3-4h | Low | None |
| 3 | TempProvider Support | ~200 | 6-8h | Medium | None |
| 4 | Force Kill | ~40 | 2-3h | Low | None |

**Total**: ~440 LOC, 15-21 hours (2-3 days)

### Testing Strategy

**For Each PR**:
1. Unit tests for new methods
2. Integration tests with real Vagrant VMs (VirtualBox)
3. Manual testing with VirtualBox, VMware Desktop, and Hyper-V
4. Verify backward compatibility with existing Vagrant workflows

**Test Coverage Targets**:
- PR #1: Test status reports with running/stopped VMs, verify metrics accuracy
- PR #2: Test config updates via context, verify Vagrantfile regeneration
- PR #3: Test temp VM creation/mount updates, verify `vagrant reload` behavior
- PR #4: Test force kill with hung VMs, verify process termination

---

## Risks and Mitigations

### Risk 1: Vagrantfile Parsing Complexity
**Risk**: Updating Vagrantfile synced folders (PR #3) requires parsing Ruby code
**Mitigation**: Use marker comments (`# BEGIN/END SYNCED FOLDERS`) to isolate sections. If complex, consider generating full Vagrantfile from scratch like Docker does with docker-compose.yml

### Risk 2: Platform-Specific Kill Commands
**Risk**: Force kill (PR #4) requires different commands for VirtualBox/VMware/Hyper-V
**Mitigation**: Start with VirtualBox support only, add VMware/Hyper-V in follow-up PRs

### Risk 3: SSH Performance for Status Reports
**Risk**: Status reports (PR #1) require multiple SSH commands, may be slow
**Mitigation**: Batch commands into single SSH session, cache results for 5-10 seconds

### Risk 4: Vagrant Reload Downtime
**Risk**: `vagrant reload` (PR #3) causes brief downtime for mount updates
**Mitigation**: Document this as expected behavior, consider `vagrant rsync` as alternative

---

## Success Metrics

### Quantitative Goals
- ✅ Vagrant passes 100% of Docker provider test suite
- ✅ Enhanced status reports return data within 2 seconds
- ✅ Config updates via context work without `vm destroy`
- ✅ Temp VM workflows match Docker behavior exactly
- ✅ Force kill terminates VMs within 5 seconds

### Qualitative Goals
- ✅ Users can choose Vagrant or Docker based on infrastructure needs, not feature limitations
- ✅ Enterprise teams with Docker restrictions have full-featured alternative
- ✅ Vagrant provider rated ⭐⭐⭐⭐⭐ (same as Docker)

---

## Future Enhancements (Out of Scope)

These are **not** required for parity but could be added later:

1. **Parallel SSH Commands**: Use GNU parallel for faster status collection
2. **Vagrantfile Templates**: Use Tera templates like Docker (more maintainable)
3. **Provider Auto-Detection**: Detect VirtualBox vs VMware automatically
4. **Metrics Caching**: Cache resource metrics to reduce SSH overhead
5. **Health Check Endpoints**: Poll HTTP endpoints for service health instead of systemctl

---

## Appendix A: Feature Comparison (Before vs After)

| Feature | Before | After | Docker Parity |
|---------|--------|-------|---------------|
| **Enhanced Status Reports** | ❌ Returns error | ✅ CPU/Memory/Disk/Services | ✅ 100% |
| **TempProvider Support** | ❌ Not implemented | ✅ Full support | ✅ 100% |
| **ProviderContext Support** | ⚠️ Passes via env var | ✅ Regenerates Vagrantfile | ✅ 100% |
| **Force Kill** | ⚠️ Calls destroy() | ✅ True force kill | ✅ 100% |
| **Multi-Instance** | ✅ Works | ✅ Works | ✅ 100% |
| **Provisioning** | ✅ Works | ✅ Works | ✅ 100% |
| **SSH** | ✅ Works | ✅ Works | ✅ 100% |
| **Logs** | ✅ Works | ✅ Works | ✅ 100% |
| **Overall Rating** | ⭐⭐⭐⭐ (40% advanced) | ⭐⭐⭐⭐⭐ (100% advanced) | ✅ Full Parity |

---

## Appendix B: Code Organization

**New Files** (None - all changes in existing files):
- All code goes in `rust/vm-provider/src/vagrant/provider.rs`

**Modified Files**:
- `rust/vm-provider/src/vagrant/provider.rs` (~440 LOC added)

**Tests**:
- `rust/vm-provider/src/vagrant/provider_tests.rs` (new file, ~300 LOC)

---

## Conclusion

This proposal provides a **clear, incremental path** to bring Vagrant to full Docker parity. With **4 PRs totaling ~440 LOC and 15-21 hours of work**, the Vagrant provider will support all advanced features:

1. ✅ Enhanced status reports with real-time metrics
2. ✅ Full ProviderContext support with config regeneration
3. ✅ TempProvider for `vm temp` workflows
4. ✅ Proper force-kill distinct from destroy

**Impact**: Enterprise teams using VirtualBox, VMware, or Hyper-V will have a **first-class VM provider experience** with no feature limitations compared to Docker.

**Recommendation**: Approve and implement in priority order (PR #1 → #2 → #3 → #4) to deliver incremental value while maintaining stability.
