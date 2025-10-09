# Proposal: Refactor `docker/lifecycle.rs` into Focused Modules

## Status
**Proposed** - Not yet implemented

## Reviewers' Findings & Adjustments

**Critical implementation concerns identified**:

1. **Cross-module helper dependencies**: Functions like `prepare_config_for_build()` and `resolve_target_container()` are called from multiple flows (create, start, restart, provision). Need explicit dependency mapping to avoid cyclic deps.

2. **API stability requires `impl` blocks**: The proposal must use `impl LifecycleOperations` blocks in each module, NOT free functions, to maintain method-based API. Re-exports alone will change call signatures.

3. **2.5k LOC move requires incremental plan**: Single-PR migration will destroy git blame and create merge hell. Need phased, mechanically-scoped patches.

4. **Some modules still too large**: Provisioning at ~600 LOC may still be "big ball of mud". Consider sub-splitting package management.

**Adjustments made**:
- Added explicit dependency call-chain mapping
- Clarified `impl` block structure (not free functions)
- Added incremental migration plan with PR boundaries
- Split provisioning into packages + orchestration
- Added git-blame preservation strategy

## Problem Statement

The `vm-provider/src/docker/lifecycle.rs` file has grown to **2,559 lines of code** with **76+ functions**, making it:

- **Hard to maintain**: Single file handles creation, provisioning, execution, status, health checks, and utilities
- **Difficult to test**: Cannot easily test individual components in isolation
- **Merge conflict prone**: Multiple developers editing the same large file
- **High cognitive load**: New contributors must understand entire lifecycle in one go
- **Violates SRP**: Single Responsibility Principle - one file doing too much

### Current Metrics
- **Total LOC**: 2,559
- **Public methods**: 26
- **Private helpers**: 50+
- **Traits implemented**: 1 (TempProvider)
- **Responsibilities**: 7+ distinct areas

## Proposed Solution

Break up `lifecycle.rs` into a module directory with focused, single-responsibility files:

```
vm-provider/src/docker/
├── lifecycle/
│   ├── mod.rs              - Main struct, constructor, module declarations
│   ├── helpers.rs          - Naming, validation, disk checks (NO dependencies)
│   ├── health.rs           - Service health checks (NO dependencies)
│   ├── packages.rs         - Package management (pipx, pip, npm, cargo)
│   ├── provisioning.rs     - Provisioning orchestration (depends: helpers, packages)
│   ├── creation.rs         - Container creation (depends: helpers, provisioning)
│   ├── execution.rs        - Start/stop/restart/kill (depends: helpers)
│   ├── interaction.rs      - SSH/exec/logs (depends: helpers)
│   └── status.rs           - Status reporting & listing (depends: helpers, health)
└── lifecycle.rs            - (deleted after ALL modules migrated)
```

**Dependency order** (bottom-up, no cycles):
```
helpers.rs (leaf)    health.rs (leaf)
    ↓                     ↓
packages.rs           status.rs
    ↓
provisioning.rs
    ↓
creation.rs
    ↓
execution.rs    interaction.rs
```

## Detailed Module Breakdown

### `lifecycle/mod.rs` (~150 LOC)
**Responsibility**: Module coordination and struct definition ONLY

**Contents**:
- `LifecycleOperations<'a>` struct definition
- Constructor `new()`
- Module declarations (`mod helpers;`, etc.)
- `TempProvider` trait implementation (delegating to impl blocks in submodules)
- **NO function re-exports** - methods stay on struct

**Example**:
```rust
//! Docker container lifecycle management operations.

// Module declarations in dependency order
mod helpers;
mod health;
mod packages;
mod provisioning;
mod creation;
mod execution;
mod interaction;
mod status;

use std::path::PathBuf;
use vm_config::config::VmConfig;

/// Main lifecycle operations struct
pub struct LifecycleOperations<'a> {
    pub config: &'a VmConfig,
    pub temp_dir: &'a PathBuf,
    pub project_dir: &'a PathBuf,
}

impl<'a> LifecycleOperations<'a> {
    /// Constructor - only public method in mod.rs
    pub fn new(config: &'a VmConfig, temp_dir: &'a PathBuf, project_dir: &'a PathBuf) -> Self {
        Self { config, temp_dir, project_dir }
    }
}

// TempProvider trait implementation (delegates to creation/execution modules)
impl<'a> TempProvider for LifecycleOperations<'a> {
    fn create_temp_vm(&self, state: &TempVmState) -> Result<()> {
        // Implementation delegates to creation module's impl block
        self.create_temp_container(state)
    }
    // ... other trait methods
}
```

**Critical**: Each module contributes methods via `impl<'a> LifecycleOperations<'a> { ... }` blocks.
No free functions, no re-exports. API stays method-based.

---

### `lifecycle/helpers.rs` (~200 LOC)
**Responsibility**: Utility functions for naming, validation, and system checks

**Lines moved**: 40-50 (constants), 129-260, 1705-1801

**Structure**:
```rust
//! Helper utilities for lifecycle operations
use super::LifecycleOperations;
use vm_config::config::VmConfig;
use vm_core::error::Result;

// Constants (moved from top of lifecycle.rs)
pub(super) const DEFAULT_PROJECT_NAME: &str = "vm-project";
pub(super) const CONTAINER_SUFFIX: &str = "-dev";
pub(super) const HIGH_MEMORY_THRESHOLD: u32 = 8192;
// ... etc

impl<'a> LifecycleOperations<'a> {
    /// Extract project name from config or default
    pub fn project_name(&self) -> &str {
        self.config
            .project
            .as_ref()
            .and_then(|p| p.name.as_deref())
            .unwrap_or(DEFAULT_PROJECT_NAME)
    }

    /// Generate default container name
    pub fn container_name(&self) -> String {
        format!("{}{}", self.project_name(), CONTAINER_SUFFIX)
    }

    /// Generate container name with instance suffix
    pub fn container_name_with_instance(&self, instance_name: &str) -> String {
        format!("{}-{}", self.project_name(), instance_name)
    }

    /// Resolve target container (from Option or default)
    pub fn resolve_target_container(&self, container: Option<&str>) -> Result<String> {
        // Lines 1705-1800
    }

    /// Get sync directory path
    pub fn get_sync_directory(&self) -> String {
        // Lines 1801-1810
    }

    // Private validation helpers (pub(super) for cross-module use)
    pub(super) fn check_memory_allocation(&self, vm_config: &vm_config::config::VmSettings) {
        // Lines 147-163
    }

    pub(super) fn check_daemon_is_running(&self) -> Result<()> {
        // Lines 180-262
    }
}

// Platform-specific disk checks (pub(super) for creation module)
#[cfg(unix)]
pub(super) fn check_disk_space_unix() { /* ... */ }

#[cfg(windows)]
pub(super) fn check_disk_space_windows() { /* ... */ }
```

**Visibility strategy**:
- Public methods (`pub`) - External API, called from outside crate
- Super-public helpers (`pub(super)`) - Used by other lifecycle modules
- Private helpers - Only used within helpers.rs

**Shared by**: creation, provisioning, execution, interaction, status

---

### `lifecycle/creation.rs` (~500 LOC)
**Responsibility**: Container creation and setup

**Functions moved** (lines 263-975):
- `create_container()` - Default creation
- `create_container_with_context()` - Creation with custom context
- `create_container_with_instance()` - Multi-instance creation
- `create_container_with_instance_and_context()` - Multi-instance with context
- `handle_existing_container()` - Existing container conflict resolution
- `handle_existing_container_with_instance()` - Instance conflict resolution
- `prepare_config_for_build()` - Config preparation
- `prepare_config_for_copy()` - Config copying
- `prepare_temp_config()` - Temp config setup
- `prepare_and_copy_config()` - Config orchestration

**Key workflows**:
1. Check for existing containers
2. Prepare configuration files
3. Build Docker image
4. Generate docker-compose.yml
5. Start container
6. Hand off to provisioning

---

### `lifecycle/packages.rs` (~250 LOC) **[NEW - Split from provisioning]**
**Responsibility**: Package management logic ONLY

**Lines moved**: 72-127 (pipx helpers)

**Structure**:
```rust
//! Package management utilities for pip/pipx/npm/cargo
use super::LifecycleOperations;
use serde_json::Value;
use std::collections::HashSet;

impl<'a> LifecycleOperations<'a> {
    /// Extract pipx managed packages
    pub(super) fn extract_pipx_managed_packages(&self, pipx_json: &Value) -> HashSet<String> {
        // Lines 72-87
    }

    /// Get pipx JSON output
    pub(super) fn get_pipx_json(&self) -> Result<Option<Value>> {
        // Lines 91-107
    }

    /// Categorize pipx packages (container vs host)
    pub(super) fn categorize_pipx_packages(&self, pipx_json: &Value) -> Vec<String> {
        // Lines 111-127
    }
}
```

**Why separate?**: Provisioning was 600 LOC - still "big ball of mud". Package management is orthogonal to Ansible provisioning.

---

### `lifecycle/provisioning.rs` (~350 LOC) **[Slimmed down]**
**Responsibility**: Provisioning orchestration (Ansible playbooks, container setup)

**Lines moved**: 812-1296 (minus package helpers)

**Structure**:
```rust
//! Container provisioning orchestration
use super::{LifecycleOperations, packages};
use crate::context::ProviderContext;

impl<'a> LifecycleOperations<'a> {
    /// Re-provision existing container (public API)
    pub fn provision_existing(&self, container: Option<&str>) -> Result<()> {
        // Lines 1297-1381
    }

    /// Internal provisioning with context
    pub(super) fn provision_container_with_context(&self, context: &ProviderContext) -> Result<()> {
        // Lines 817-889
        // Uses package helpers via self.get_pipx_json(), etc.
    }

    /// Provision with instance name
    pub(super) fn provision_container_with_instance(&self, instance_name: &str) -> Result<()> {
        // Lines 890-975
    }

    // Private helpers
    fn wait_for_container_ready(&self, container_name: &str) -> Result<()> {
        // Ansible provisioning, readiness polling
    }

    fn execute_ansible_playbook(&self, container_name: &str) -> Result<()> {
        // Ansible execution logic
    }
}
```

**Dependencies**: Uses `packages` module for pipx logic, `helpers` for naming

**Key workflows**:
1. Wait for container readiness
2. Copy temp config into container
3. Execute Ansible provisioning
4. Delegate package categorization to `packages` module
5. Verify provisioning success

---

### `lifecycle/execution.rs` (~300 LOC)
**Responsibility**: Container lifecycle execution (start/stop/restart/kill)

**Functions moved** (lines 976-1296, 1684-1704):
- `start_container()` - Start stopped container
- `start_container_with_context()` - Start with context
- `stop_container()` - Graceful shutdown
- `restart_container()` - Restart container
- `restart_container_with_context()` - Restart with context
- `destroy_container()` - Remove container and cleanup
- `kill_container()` - Force kill container

**Key workflows**:
1. Validate container exists
2. Execute docker-compose commands
3. Handle errors and retries
4. Cleanup resources

---

### `lifecycle/interaction.rs` (~250 LOC)
**Responsibility**: User interaction with containers (SSH/exec/logs)

**Functions moved** (lines 1070-1296):
- `ssh_into_container()` - SSH session handling
- `exec_in_container()` - Execute commands in container
- `show_logs()` - Display container logs

**Key workflows**:
1. Resolve container name
2. Build docker exec/attach commands
3. Handle TTY allocation
4. Stream output to user

---

### `lifecycle/status.rs` (~400 LOC)
**Responsibility**: Container status reporting and listing

**Functions moved** (lines 1382-2080):
- `list_containers()` - List all containers
- `list_containers_with_stats()` - List with resource stats
- `get_status_report()` - Comprehensive status report
- `get_container_info()` - Docker inspect parsing
- `get_resource_usage()` - Docker stats parsing
- `get_host_port()` - Port mapping extraction
- Resource formatting helpers

**Key workflows**:
1. Query docker for container info
2. Parse JSON responses
3. Aggregate resource usage
4. Format for display

---

### `lifecycle/health.rs` (~200 LOC)
**Responsibility**: Service-specific health checks

**Functions moved** (lines 2081-2215):
- `check_postgres_status()` - PostgreSQL health check via `pg_isready`
- `check_redis_status()` - Redis health check via `redis-cli ping`
- `check_mongodb_status()` - MongoDB health check via `mongosh`

**Pattern**:
```rust
pub(super) fn check_postgres_status(container_name: &str, port: u16) -> ServiceStatus {
    // Execute health check command in container
    // Parse output
    // Return ServiceStatus with metrics/errors
}
```

**Note**: These are `pub(super)` so only accessible within `lifecycle` module, called by `status.rs`.

---

## API Compatibility

### External API: **100% Backward Compatible**

All existing code using `LifecycleOperations` will continue to work without changes:

```rust
// Before and After - SAME API
let lifecycle = LifecycleOperations::new(&config, &temp_dir, &project_dir);
lifecycle.create_container()?;
lifecycle.start_container(None)?;
lifecycle.get_status_report(None)?;
```

### Internal Changes

**Parent module** (`docker/mod.rs`):
```rust
// Before
mod lifecycle;
pub use lifecycle::LifecycleOperations;

// After
pub mod lifecycle;  // Changed from `mod` to `pub mod`
pub use lifecycle::LifecycleOperations;
```

**Import statement changes**: None required for external consumers.

---

## Implementation Notes

### Function Visibility Strategy

- **Public** (`pub`): External API - methods called from outside `vm-provider` crate
- **Super-public** (`pub(super)`): Internal API - methods called within `lifecycle` module
- **Private**: Implementation details - only used within same file

### Module Dependencies & Call Chains

**Dependency graph** (avoid cycles):
```
mod.rs
  ├─> helpers.rs      (no dependencies) ← LEAF
  ├─> health.rs       (no dependencies) ← LEAF
  ├─> packages.rs     (depends: helpers)
  ├─> provisioning.rs (depends: helpers, packages)
  ├─> creation.rs     (depends: helpers, provisioning)
  ├─> execution.rs    (depends: helpers)
  ├─> interaction.rs  (depends: helpers)
  └─> status.rs       (depends: helpers, health)
```

**Critical cross-module helper usage**:

| Helper Function | Current Callers | Module Placement | Visibility |
|-----------------|----------------|------------------|------------|
| `resolve_target_container()` | create, start, restart, provision, ssh, exec, logs | helpers.rs | `pub` |
| `container_name()` / `container_name_with_instance()` | ALL modules | helpers.rs | `pub` |
| `check_daemon_is_running()` | create, start | helpers.rs | `pub(super)` |
| `prepare_config_for_build()` | create (only) | creation.rs | private |
| `prepare_temp_config()` | create, provision | creation.rs | `pub(super)` |
| `extract_pipx_managed_packages()` | provision (only) | packages.rs | private |
| `categorize_pipx_packages()` | provision (only) | packages.rs | private |

**Resolution strategy**:
- Widely-used helpers (3+ callers) → `helpers.rs` with appropriate visibility
- Module-specific helpers (1-2 callers) → stay in owning module
- Shared config prep → `creation.rs` with `pub(super)` for provisioning access

### Shared State Access

All modules receive `&self` reference to `LifecycleOperations`, providing access to:
- `self.config` - VM configuration
- `self.temp_dir` - Temporary directory path
- `self.project_dir` - Project directory path

### Testing Strategy

**Current**: Tests must import entire `lifecycle.rs`

**After**: Can test individual modules:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_naming() {
        // Test only helpers module
    }

    #[test]
    fn test_health_checks() {
        // Test only health module
    }
}
```

---

## Benefits

### Maintainability
- ✅ Each module has clear, single responsibility
- ✅ Easier to locate relevant code (~300 LOC vs 2,559 LOC per file)
- ✅ Reduced cognitive load for understanding individual operations

### Testability
- ✅ Can unit test modules independently
- ✅ Easier to mock dependencies between modules
- ✅ Test isolation prevents cascading failures

### Collaboration
- ✅ Fewer merge conflicts (8 files vs 1 large file)
- ✅ Clearer code ownership per module
- ✅ Parallel development on different lifecycle aspects

### Onboarding
- ✅ New contributors can understand one module at a time
- ✅ Clear file names indicate purpose
- ✅ Module documentation focuses on specific domain

### Performance
- ✅ **No runtime changes** - same compiled code
- ✅ **No API changes** - existing callers unchanged
- ✅ Compiler optimizations identical (inlining still works)

---

## Non-Goals

This refactoring **does not**:
- ❌ Change functionality or fix bugs
- ❌ Modify external API surface
- ❌ Add new features
- ❌ Change error handling patterns
- ❌ Alter trait implementations
- ❌ Require updates to calling code

---

## Incremental Migration Plan

**Critical**: Do NOT migrate in one PR. Split into mechanically-scoped patches to preserve git blame and enable review.

### PR #1: Infrastructure Setup (helpers + health) **[~400 LOC]**
**Goal**: Establish module structure with leaf nodes (no dependencies)

**Changes**:
1. Create `lifecycle/` directory
2. Create `lifecycle/mod.rs` with struct definition
3. Move helpers to `lifecycle/helpers.rs` (lines 40-50, 129-260, 1705-1801)
4. Move health checks to `lifecycle/health.rs` (lines 2081-2215)
5. Update `docker/mod.rs` to use `pub mod lifecycle`
6. Keep `lifecycle.rs` intact (mark as `#[deprecated]` with comment)

**Tests**: Run full test suite, all should pass

**Git blame**: Preserved for 90% of lifecycle.rs (only moved ~15%)

**Reviewability**: ~400 LOC mechanical move, clear module boundaries

---

### PR #2: Package Management Split **[~250 LOC]**
**Goal**: Extract package logic before provisioning

**Changes**:
1. Create `lifecycle/packages.rs`
2. Move pipx helpers (lines 72-127)
3. Update `mod.rs` to declare `packages` module

**Dependencies**: Only helpers (already moved in PR #1)

**Tests**: Run provision tests specifically

**Git blame**: Preserved for package helpers

---

### PR #3: Provisioning Orchestration **[~350 LOC]**
**Goal**: Move Ansible provisioning logic

**Changes**:
1. Create `lifecycle/provisioning.rs`
2. Move provision functions (lines 812-1296, minus package helpers)
3. Update calls to use `self.get_pipx_json()` from packages module

**Dependencies**: helpers, packages (both already moved)

**Tests**: Run provision + creation tests

**Git blame**: Preserved for provisioning logic

---

### PR #4: Container Creation **[~500 LOC]**
**Goal**: Move creation workflows

**Changes**:
1. Create `lifecycle/creation.rs`
2. Move create_container* variants (lines 263-811)
3. Move config preparation helpers
4. Update calls to `self.provision_container_with_context()`

**Dependencies**: helpers, provisioning (both already moved)

**Tests**: Run creation + integration tests

**Git blame**: Preserved for creation logic

---

### PR #5: Execution Operations **[~300 LOC]**
**Goal**: Move start/stop/restart/kill

**Changes**:
1. Create `lifecycle/execution.rs`
2. Move execution functions (lines 976-1296, 1684-1704)
3. No cross-module dependencies besides helpers

**Tests**: Run start/stop/restart tests

**Git blame**: Preserved for execution logic

---

### PR #6: Interaction & Status **[~650 LOC]**
**Goal**: Complete migration with SSH/exec/logs and status reporting

**Changes**:
1. Create `lifecycle/interaction.rs` (lines 1070-1296)
2. Create `lifecycle/status.rs` (lines 1382-2080)
3. Update status module to call health checks from `health` module
4. Update `TempProvider` trait impl in `mod.rs`

**Dependencies**: helpers, health (already moved)

**Tests**: Run full integration suite

**Git blame**: Preserved for interaction/status logic

---

### PR #7: Cleanup & Delete **[Delete 2,559 LOC]**
**Goal**: Remove old file, finalize migration

**Changes**:
1. Verify ALL tests pass
2. Run `cargo clippy --all-features`
3. Delete `lifecycle.rs` (now empty/deprecated)
4. Update CHANGELOG

**Tests**: Full test suite, Docker integration tests

**Git history**: All blame preserved in new modules

---

### Migration Safety Checklist

**Before each PR**:
- [ ] Create feature branch: `refactor/lifecycle-PR{N}-{module-name}`
- [ ] Run `cargo test --package vm-provider` (must pass)
- [ ] Run `cargo clippy --package vm-provider` (no new warnings)

**During review**:
- [ ] Verify `impl<'a> LifecycleOperations<'a>` blocks (not free functions)
- [ ] Check visibility (`pub` vs `pub(super)` vs private)
- [ ] Confirm no cyclic dependencies

**After merge**:
- [ ] Run full integration suite with Docker
- [ ] Check git blame in new modules (should show original authors)
- [ ] Update PR tracking issue

---

## Risks & Mitigation

### Risk: Breaking existing code
**Mitigation**:
- Maintain 100% API compatibility
- All public methods remain on `LifecycleOperations`
- Comprehensive test coverage before/after

### Risk: Circular dependencies between modules
**Mitigation**:
- Clear dependency hierarchy (helpers at bottom)
- Use `pub(super)` to prevent external access
- Keep module interfaces thin

### Risk: Incorrect function placement
**Mitigation**:
- Follow Single Responsibility Principle
- Group by domain (creation, execution, etc.) not by technical layer
- Review module boundaries during implementation

### Risk: Lost context from splitting code
**Mitigation**:
- Add module-level documentation (`//!` comments)
- Cross-reference related functions in comments
- Preserve existing function-level documentation

---

## Success Criteria

1. ✅ All existing tests pass without modification
2. ✅ No changes to external API (consumers don't need updates)
3. ✅ Each module file < 700 LOC
4. ✅ `cargo clippy` passes with no new warnings
5. ✅ Compilation time unchanged or improved
6. ✅ Documentation builds successfully

---

## Future Enhancements (Out of Scope)

After this refactor, future improvements become easier:

- **Better error types**: Each module could have domain-specific errors
- **Async provisioning**: Provisioning module could use async/await
- **Plugin architecture**: Health checks could be registered dynamically
- **Integration testing**: Mock individual modules for testing
- **Performance profiling**: Easier to identify bottlenecks per module

---

## References

- **Current file**: `rust/vm-provider/src/docker/lifecycle.rs` (2,559 LOC)
- **Related modules**: `docker/compose.rs`, `docker/build.rs`, `docker/command.rs`
- **Trait implementations**: `Provider` (in `docker/mod.rs`), `TempProvider` (in `lifecycle.rs`)
- **External consumers**: `vm/src/commands/vm_ops.rs`, `vm-temp/src/temp_ops.rs`

---

## Changelog Entry

```markdown
### Changed
- **Refactored docker lifecycle module**: Split `docker/lifecycle.rs` (2,559 LOC) into 8 focused modules for improved maintainability
  - `lifecycle/creation.rs` - Container creation logic
  - `lifecycle/provisioning.rs` - Provisioning and package management
  - `lifecycle/execution.rs` - Start/stop/restart/kill operations
  - `lifecycle/interaction.rs` - SSH/exec/logs operations
  - `lifecycle/status.rs` - Status reporting and listing
  - `lifecycle/health.rs` - Service health checks
  - `lifecycle/helpers.rs` - Utility functions
  - `lifecycle/mod.rs` - Module coordination
- **No API changes**: All existing code continues to work without modification
- **Improved testability**: Individual modules can now be tested in isolation
```

---

## Approval

- [ ] Approved by: _______________
- [ ] Date: _______________
- [ ] Implementation branch: `refactor/lifecycle-module-split`
