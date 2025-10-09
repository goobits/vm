# Proposal: Legacy Code Cleanup

**Status**: Draft
**Owner**: Dev Experience
**Target Release**: 2.2.0

---

## Problem

The codebase has accumulated technical debt that needs cleanup:

1. **Dead code with suppressed warnings** - 10+ files with `#[allow(dead_code)]` hiding unused functions
2. **Legacy git artifacts** - `legacy/main` branch exists from repository history rewrite
3. **Deprecated config fields** - `env_template_path: null` in 10 YAML files
4. **Confusing terminology** - "legacy mode" in install.sh, "Legacy check target" in Makefile
5. **Outdated compatibility layers** - MongoDB `mongo` client check (deprecated 2022), Windows PowerShell 5.x paths (2016)

These create maintenance burden and confuse contributors.

---

## Proposed Solution

### 1. Dead Code Audit

Run clippy to identify and remove unused code:

```bash
cd rust && cargo clippy --workspace -- -D dead_code
```

For each finding:
- **Remove** if truly unused
- **Document** if needed for API stability (add comment explaining why)
- **No suppression** without justification

**Files to check:**
- `rust/vm-provider/src/docker/command.rs`
- `rust/vm-provider/src/docker/compose.rs`
- `rust/vm/src/commands/vm_ops/mod.rs` (unused exports)
- `rust/vm/src/service_registry.rs`
- `rust/vm/src/service_manager.rs`
- `rust/vm/tests/vm_ops/multi_instance_tests.rs` (unused imports)
- `rust/vm/tests/vm_ops/provider_parity_tests.rs` (unused imports)

### 2. Git Repository Cleanup

```bash
# Verify history is preserved in main
git log legacy/main --oneline | head -20

# Delete legacy branch
git branch -D legacy/main

# Clean up filter-repo artifacts
rm -rf .git/filter-repo/
```

### 3. Configuration File Cleanup

Remove deprecated `env_template_path: null` from:
- `configs/services/postgresql.yaml`
- `configs/services/redis.yaml`
- `configs/services/mongodb.yaml`
- `configs/services/docker.yaml`
- `configs/aliases.yaml`
- `configs/ports.yaml`
- `configs/languages/cargo_packages.yaml`
- `configs/languages/npm_packages.yaml`
- `configs/languages/pip_packages.yaml`

### 4. Terminology Fixes

**install.sh:**
```bash
# Before:
./install.sh --build-from-source  # Build from source (legacy mode)

# After:
./install.sh --build-from-source  # Build from source
```

**Makefile:**
```make
# Before:
# Legacy check target
check: fmt-fix clippy test

# After:
# Run formatting, linting, and tests
check: fmt-fix clippy test
```

### 5. Remove Outdated Compatibility Checks

**MongoDB detector** (`rust/vm-config/src/detector/tools.rs:96`):
```rust
// Before: Checks for both mongosh and legacy mongo
// After: Only check mongosh (mongo deprecated in MongoDB 6.0, 2022)
```

**Windows PowerShell** (`rust/vm-platform/src/providers/shells.rs:135`):
```rust
// Remove fallback to WindowsPowerShell (PowerShell 5.x from 2016)
// Modern PowerShell 7+ uses different path
```

---

## Implementation Plan

Single PR with sections:
1. Run dead code audit, remove/document findings
2. Delete legacy git branch and artifacts
3. Remove `env_template_path: null` from 10 files
4. Update comments in install.sh and Makefile
5. Remove outdated tool detection (MongoDB, PowerShell)

---

## Success Metrics

- Zero `#[allow(dead_code)]` suppressions without documentation
- No `legacy/main` branch
- No deprecated null fields in config templates
- CI enforces `clippy -D dead_code` going forward

---

## Risks

- **Dead code might be needed**: Document why kept or add feature flag
- **Breaking tool detection**: Test on systems with old tools installed

---

## Non-Goals

- No breaking changes to user-facing APIs
- No removal of backward compatibility for recent versions
- No refactoring of working code

---

## Estimated Effort

2-3 days (mostly verifying dead code can be safely removed)
