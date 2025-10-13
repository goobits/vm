# Security & Safety Defaults Improvements

**Status:** Open
**Impact:** Reduces security risks and prevents data loss

---

## Problem

Current default configurations expose security vulnerabilities and data loss risks:

1. **Port Binding Exposure**: Default `port_binding: 0.0.0.0` exposes all VM ports to the network, creating security risks on public WiFi or shared networks
2. **Hardcoded Database Credentials**: All databases use `postgres:postgres` / `root:root`, making them trivially exploitable if ports are exposed
3. **No Backup Protection**: `backup_on_destroy: false` by default makes it easy to accidentally lose database data
4. **Missing Swap Configuration**: VMs have no swap space, leading to OOM kills under memory pressure

---

## Proposed Solutions

### Solution 1: Secure Port Binding by Default

**Change `configs/defaults.yaml`:**
```yaml
vm:
  port_binding: 127.0.0.1  # Localhost only, not 0.0.0.0
```

**Benefits:**
- Prevents accidental network exposure
- Users must explicitly opt-in to LAN access via project config
- No breaking changes (project configs can override)

### Solution 2: Generate Random Database Passwords

**Implementation approach:**

**Option A: Generate on First Service Start**
- When service starts for first time, generate random password
- Store in `~/.vm/secrets/{service-name}.env`
- Inject into containers and display to user
- Reuse same password on subsequent starts

**Option B: Generate Per-Project**
- Each project gets unique passwords
- Store in `~/.vm/state/{project-name}/secrets.env`
- Show in `vm status` output

**Recommended: Option A** (simpler, more secure)

**Files to modify:**
- `rust/vm/src/service_manager.rs` - Password generation logic
- `rust/vm-provider/src/docker/compose.rs` - Inject generated passwords
- `rust/vm-config/src/global_config.rs` - Add password storage paths

### Solution 3: Enable Database Backups by Default

**Change service defaults:**
```rust
// In ServiceConfig::default()
pub backup_on_destroy: bool,  // Change default to true for databases
```

**Add global config option:**
```yaml
# ~/.vm/config.yaml
backups:
  enabled: true              # Global toggle
  path: ~/.vm/backups        # Where to store backups
  keep_count: 5              # Rotate old backups
  databases_only: true       # Only backup databases by default
```

### Solution 4: Add Default Swap Configuration

**Change `configs/defaults.yaml`:**
```yaml
vm:
  memory: 8192
  cpus: 6
  swap: 2048         # 2GB swap
  swappiness: 60     # Standard Linux default
```

**Update Docker provider to apply swap:**
```rust
// In docker/compose.rs
mem_swappiness: config.vm.swappiness,
memswap_limit: memory_limit + swap_limit,
```

---

## Implementation Checklist

### Port Binding Security
- [ ] Update `configs/defaults.yaml` to use `127.0.0.1`
- [ ] Add documentation explaining security implications
- [ ] Add example in README for exposing to LAN when needed
- [ ] Test that localhost binding works correctly
- [ ] Verify project configs can override to `0.0.0.0`

### Random Database Passwords
- [ ] Create `~/.vm/secrets/` directory structure
- [ ] Add password generation utility function
- [ ] Modify `service_manager.rs` to generate passwords on first start
- [ ] Store generated passwords in secrets directory
- [ ] Update `compose.rs` to read and inject passwords
- [ ] Display passwords in `vm status` output
- [ ] Add `vm db credentials` command to show passwords
- [ ] Test PostgreSQL with generated password
- [ ] Test Redis with generated password
- [ ] Test MongoDB with generated password
- [ ] Test MySQL with generated password
- [ ] Document password location and retrieval

### Database Backups
- [ ] Change `ServiceConfig::backup_on_destroy` default to `true`
- [ ] Add global backups configuration section
- [ ] Implement backup rotation (keep N backups)
- [ ] Test backup creation on `vm destroy`
- [ ] Add `vm db backup` command for manual backups
- [ ] Add `vm db restore` command
- [ ] Document backup location and restoration process
- [ ] Add option to skip backup: `vm destroy --no-backup`

### Swap Configuration
- [ ] Add `swap` field to `VmSettings` struct
- [ ] Add `swappiness` field to `VmSettings` struct
- [ ] Update `configs/defaults.yaml` with swap defaults
- [ ] Implement swap in Docker provider (`compose.rs`)
- [ ] Implement swap in Tart provider (if applicable)
- [ ] Test VM with swap under memory pressure
- [ ] Verify swap is working: `vm exec free -h`
- [ ] Document swap configuration options

---

## Success Criteria

### Port Binding
- [ ] New VMs bind to `127.0.0.1` by default
- [ ] Services are not accessible from other machines on LAN
- [ ] Project configs can override to `0.0.0.0` when needed
- [ ] Documentation clearly explains security implications

### Database Passwords
- [ ] Each database service gets a unique random password
- [ ] Passwords are stored securely in `~/.vm/secrets/`
- [ ] Passwords persist across VM restarts
- [ ] Users can view passwords via `vm status` or `vm db credentials`
- [ ] Environment variables (DATABASE_URL, etc.) use generated passwords
- [ ] No hardcoded `postgres:postgres` or `root:root` in generated URLs

### Backups
- [ ] Database services back up automatically on `vm destroy`
- [ ] Backups are stored in `~/.vm/backups/{service-name}/`
- [ ] Backup rotation works (keeps N most recent)
- [ ] Restore command successfully restores from backup
- [ ] Users can opt-out with `--no-backup` flag

### Swap
- [ ] VMs have 2GB swap by default
- [ ] Swappiness is set to 60
- [ ] VMs handle memory pressure better (no immediate OOM kills)
- [ ] Swap settings can be customized in config

---

## Related Issues

### Configuration Hierarchy
These changes affect the configuration hierarchy:
1. Hardcoded defaults (in code)
2. Global config (`~/.vm/config.yaml`)
3. Project config (`vm.yaml`)

Ensure project configs can override all security settings when needed.

### Breaking Changes
- **Port binding change** could break workflows that rely on LAN access
- **Mitigation**: Document migration, add warning on first run after upgrade
- **Random passwords** could break scripts that assume `postgres:postgres`
- **Mitigation**: Add `use_default_passwords: true` option for compatibility

---

## Benefits

**Security:**
- Prevents accidental exposure of services on public networks
- Eliminates trivially exploitable default passwords
- Reduces attack surface for development VMs

**Reliability:**
- Swap prevents OOM kills during memory spikes
- VMs are more stable under load
- Better matches production environments

**Data Safety:**
- Automatic backups prevent accidental data loss
- Easy recovery from mistakes
- Peace of mind when experimenting

**Developer Experience:**
- More secure by default without extra configuration
- Passwords generated automatically, no need to set them manually
- Better defaults reduce need for project-specific overrides
