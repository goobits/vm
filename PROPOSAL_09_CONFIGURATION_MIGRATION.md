# Proposal: Configuration Migration System

**Status**: Draft
**Owner**: Dev Experience
**Target Release**: 2.2.0

---

## Problem

Configuration files have moved to new locations, but the code maintains dual-path checks for backward compatibility:

**4 affected paths:**
1. `config.yaml`: `~/.vm/config.yaml` (new) vs `~/.config/vm/global.yaml` (old)
2. `ports.json`: `~/.vm/ports.json` (new) vs `~/.vm/port-registry.json` (old)
3. `services.json`: `~/.vm/services.json` (new) vs `~/.vm/service_state.json` (old)
4. `temp-vms.json`: `~/.vm/temp-vms.json` (new) vs `~/.vm/temp-vm.state` (old)

**Current behavior** (`rust/vm-core/src/user_paths.rs:67-153`):
```rust
pub fn global_config_path() -> Result<PathBuf> {
    let new_path = vm_state_dir()?.join("config.yaml");
    if new_path.exists() { return Ok(new_path); }

    let old_path = user_config_dir()?.join("global.yaml");
    if old_path.exists() { return Ok(old_path); }

    Ok(new_path)  // Return new even if doesn't exist
}
```

**Problems:**
- Every config operation checks 2 locations (I/O overhead)
- Users don't know they should migrate
- Code complexity in `user_paths.rs` (86 lines of dual-path logic)
- No automated migration path

Additionally, **deprecated config field** `persist_databases` is documented as "still supported" without sunset date.

---

## Proposed Solution

### Part 1: Migration Command

Add `vm config migrate` subcommand:

```bash
vm config migrate  # Auto-detects old files and prompts to migrate
```

**Behavior:**
```
🔍 Checking for old configuration files...

Found old configuration files:
  • ~/.config/vm/global.yaml → ~/.vm/config.yaml
  • ~/.vm/port-registry.json → ~/.vm/ports.json

Migrate these files? [Y/n]
> Y

✅ Migrated ~/.config/vm/global.yaml → ~/.vm/config.yaml
✅ Migrated ~/.vm/port-registry.json → ~/.vm/ports.json

Migration complete! Old files backed up to ~/.vm/backups/
```

**Implementation:**
- Check each old path
- If exists and new doesn't exist: copy to new location
- Backup old files to `~/.vm/backups/{timestamp}/`
- Verify new file is valid (can be loaded)
- Add migration record to `~/.vm/migration.log`

### Part 2: Deprecation Warnings

Add warnings when old paths are used:

```rust
pub fn global_config_path() -> Result<PathBuf> {
    let new_path = vm_state_dir()?.join("config.yaml");
    if new_path.exists() { return Ok(new_path); }

    let old_path = user_config_dir()?.join("global.yaml");
    if old_path.exists() {
        eprintln!("⚠️  Deprecated config location detected.");
        eprintln!("   Run 'vm config migrate' to update.");
        return Ok(old_path);
    }

    Ok(new_path)
}
```

### Part 3: Deprecate `persist_databases` Field

**Documentation update** (`docs/user-guide/configuration.md:602`):

```yaml
# DEPRECATED: Will be removed in v3.0.0 (Q2 2026)
# Use per-service persistence configuration instead
persist_databases: true

# Modern approach (use this instead):
services:
  postgresql:
    enabled: true
    persist: true  # Per-service control
```

**Add to CHANGELOG:**
```markdown
## Deprecated
- `persist_databases` top-level field (use per-service `persist` instead)
  - Removal date: v3.0.0
  - Migration: See docs/user-guide/configuration.md
```

### Part 4: Add to `vm doctor`

```bash
vm doctor

# Output includes:
Configuration files:
  ✅ Using modern config paths
  ⚠️  Old config detected at ~/.config/vm/global.yaml
      Run 'vm config migrate' to update
```

---

## Implementation Plan

### PR 1: Migration Command (Week 1)
- Add `vm config migrate` subcommand
- Implement file copying with backups
- Add migration logging
- Write tests for migration logic

### PR 2: Deprecation Warnings (Week 1)
- Add warnings to `user_paths.rs`
- Update `vm doctor` to check config locations
- Add deprecation notice to docs

### PR 3: Deprecate persist_databases (Week 1)
- Add deprecation timeline to docs
- Add CHANGELOG entry
- Create migration examples

### PR 4: Remove Old Paths (v3.0.0 - 6 months later)
- Remove dual-path checks from `user_paths.rs`
- Remove old path fallbacks
- Update tests

---

## Migration Timeline

- **v2.2.0** (Now): Ship migration tool + warnings
- **v2.2.0 - v2.9.x** (6 months): Grace period, warnings shown
- **v3.0.0** (Q2 2026): Remove old path support

---

## Success Metrics

- Users have clear migration path
- `vm config migrate` successfully moves all config files
- Deprecation warnings guide users to migration tool
- Zero dual-path checks after v3.0.0

---

## Risks & Mitigation

| Risk | Mitigation |
|------|------------|
| File corruption during migration | Create backups before any changes |
| Users ignore warnings | Add to `vm doctor` output |
| Breaking workflows | 6-month grace period before removal |

---

## Alternatives Considered

### 1. Silent Auto-Migration
**Rejected**: Users should be aware of file moves

### 2. Immediate Removal
**Rejected**: Too aggressive, breaks existing installs

### 3. Living with Dual Paths
**Rejected**: Performance overhead, code complexity

---

## Non-Goals

- Not migrating content within config files (only locations)
- Not changing config file formats
- Not removing backward compatibility for field names

---

## Estimated Effort

- Migration command: 1 day
- Deprecation warnings: 0.5 day
- Documentation: 0.5 day
- **Total: 2 days**
