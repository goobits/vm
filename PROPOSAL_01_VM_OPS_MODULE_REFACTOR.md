# Proposal: Refactor `vm/src/commands/vm_ops.rs` into Command Modules

## Status
**Proposed** - Not yet implemented

## Problem Statement

The `vm/src/commands/vm_ops.rs` file has grown to **1,507 lines of code** with **14 public command handlers** and **17 private helper functions**, making it:

- **Hard to navigate**: All VM commands in one file - developers must scroll through 1,500 LOC to find specific handlers
- **Difficult to test**: Cannot test individual commands in isolation
- **Merge conflict prone**: Multiple developers editing the same command file
- **High cognitive load**: Must understand all commands to modify one
- **Violates SRP**: Single file handling create, start, stop, destroy, SSH, exec, logs, list, status, provision

### Current Metrics
- **Total LOC**: 1,507
- **Public command handlers**: 14
- **Private helpers**: 17
- **Responsibilities**: 10+ distinct command domains

### Command Breakdown
```
Creation:        handle_create (188 LOC)
Lifecycle:       handle_start (139 LOC), handle_stop (77 LOC), handle_restart (44 LOC)
Provisioning:    handle_provision (33 LOC)
Listing:         handle_list, handle_list_enhanced, get_all_instances (141 LOC)
Destruction:     handle_destroy (115 LOC), handle_destroy_enhanced (20 LOC), cross-provider destroy (159 LOC)
Interaction:     handle_ssh (137 LOC), handle_exec (47 LOC), handle_logs (27 LOC)
Status:          handle_status (33 LOC) + display helpers (158 LOC)
Utilities:       handle_get_sync_directory (6 LOC)
Service helpers: register/unregister_vm_services_helper (35 LOC)
```

## Proposed Solution

Break up `vm_ops.rs` into a module directory with focused, command-specific files:

```
vm/src/commands/
├── vm_ops/
│   ├── mod.rs          - Module coordination, re-exports
│   ├── create.rs       - handle_create
│   ├── lifecycle.rs    - handle_start, handle_stop, handle_restart, handle_provision
│   ├── destroy.rs      - handle_destroy*, cross-provider destroy logic
│   ├── list.rs         - handle_list*, provider query logic
│   ├── status.rs       - handle_status, status display helpers
│   ├── interaction.rs  - handle_ssh, handle_exec, handle_logs
│   └── helpers.rs      - format_*, service registration, utility functions
└── vm_ops.rs           - (deleted after migration)
```

**Module size targets**: Each module 150-300 LOC (except destroy which may be ~300-350 LOC)

---

## Detailed Module Breakdown

### `vm_ops/mod.rs` (~50 LOC)
**Responsibility**: Module coordination and public API surface

**Contents**:
```rust
//! VM operation command handlers

// Module declarations
mod create;
mod destroy;
mod helpers;
mod interaction;
mod lifecycle;
mod list;
mod status;

// Re-export all public handlers for external use
pub use create::handle_create;
pub use destroy::{handle_destroy, handle_destroy_enhanced};
pub use helpers::handle_get_sync_directory;
pub use interaction::{handle_exec, handle_logs, handle_ssh};
pub use lifecycle::{handle_provision, handle_restart, handle_start, handle_stop};
pub use list::{handle_list, handle_list_enhanced};
pub use status::handle_status;
```

**Why thin mod.rs**: Keep coordination minimal - just module declarations and re-exports

---

### `vm_ops/create.rs` (~200 LOC)
**Responsibility**: VM creation command ONLY

**Lines moved**: 21-209

**Public API**:
```rust
pub async fn handle_create(
    provider: Box<dyn Provider>,
    config: VmConfig,
    global_config: GlobalConfig,
    force: bool,
    instance: Option<String>,
    verbose: bool,
) -> VmResult<()>
```

**Key workflows**:
1. Multi-instance provider detection
2. Force flag handling (destroy existing VM)
3. Provider context creation with verbose flag
4. VM creation via provider
5. Resource/service display
6. Service registration (calls `helpers::register_vm_services_helper`)

**Dependencies**: `helpers` for service registration

---

### `vm_ops/lifecycle.rs` (~280 LOC)
**Responsibility**: VM lifecycle operations (start/stop/restart/provision)

**Lines moved**: 212-513

**Public API**:
```rust
pub async fn handle_start(provider, container, config, global_config) -> VmResult<()>
pub async fn handle_stop(provider, container, config, global_config) -> VmResult<()>
pub async fn handle_restart(provider, container, config, global_config) -> VmResult<()>
pub fn handle_provision(provider, container, config) -> VmResult<()>
```

**Key workflows**:
- **Start**: Check if already running, start via provider, register services
- **Stop**: Graceful stop for project VM or force-kill specific container, unregister services
- **Restart**: Restart via provider, re-register services
- **Provision**: Re-run provisioning on existing container

**Dependencies**: `helpers` for service registration/unregistration

---

### `vm_ops/list.rs` (~200 LOC)
**Responsibility**: VM listing across providers

**Lines moved**: 516-688

**Public API**:
```rust
pub fn handle_list(provider: Box<dyn Provider>) -> VmResult<()>
pub fn handle_list_enhanced(provider, all_providers, provider_filter, verbose) -> VmResult<()>
```

**Private helpers**:
```rust
fn get_all_instances() -> VmResult<Vec<InstanceInfo>>
fn get_instances_from_provider(provider_name: &str) -> VmResult<Vec<InstanceInfo>>
fn truncate_string(s: &str, max_len: usize) -> String
fn format_status(status: &str) -> String
fn format_uptime(uptime: &Option<String>) -> String
```

**Key workflows**:
1. Query all providers (docker, tart, vagrant) for instances
2. Filter by provider name if specified
3. Display formatted table with status icons
4. Handle empty results gracefully

**Note**: Format helpers (`truncate_string`, `format_status`, `format_uptime`) could optionally move to `helpers.rs` if used elsewhere

---

### `vm_ops/destroy.rs` (~350 LOC)
**Responsibility**: VM destruction (single and cross-provider)

**Lines moved**: 699-994

**Public API**:
```rust
pub async fn handle_destroy(provider, container, config, global_config, force) -> VmResult<()>
pub async fn handle_destroy_enhanced(provider, container, config, global_config, force, all, provider_filter, pattern) -> VmResult<()>
```

**Private helpers**:
```rust
fn handle_cross_provider_destroy(all, provider_filter, pattern, force) -> VmResult<()>
fn destroy_single_instance(instance: &InstanceInfo) -> VmResult<()>
fn match_pattern(name: &str, pattern: &str) -> bool
```

**Key workflows**:
1. **Single destroy**: Confirmation prompt (unless force), destroy via provider, unregister services
2. **Cross-provider destroy**: List instances, filter by pattern, bulk destroy with progress
3. Pattern matching with wildcard support (`*`, `prefix*`, `*suffix`)

**Dependencies**: `helpers` for service unregistration, `list::get_all_instances` for cross-provider queries

**Note**: This is the largest module (~350 LOC) due to cross-provider logic complexity

---

### `vm_ops/interaction.rs` (~300 LOC)
**Responsibility**: User interaction with VMs (SSH/exec/logs)

**Lines moved**: 997-1468

**Public API**:
```rust
pub fn handle_ssh(provider, container, path, config) -> VmResult<()>
pub fn handle_exec(provider, container, command, config) -> VmResult<()>
pub fn handle_logs(provider, container, config) -> VmResult<()>
```

**Private helpers**:
```rust
fn handle_ssh_start_prompt(provider, container, relative_path, vm_name, ...) -> VmResult<Option<VmResult<()>>>
```

**Key workflows**:
- **SSH**: Connect to VM, handle "not running" with auto-start prompt, handle "doesn't exist" with create prompt
- **Exec**: Execute command in VM, display output with separators
- **Logs**: Display VM logs with header/footer

**Complex SSH error handling**:
- VM doesn't exist → offer to create
- VM not running → offer to start
- Connection errors → graceful messages

---

### `vm_ops/status.rs` (~250 LOC)
**Responsibility**: VM status reporting and display

**Lines moved**: 1196-1390

**Public API**:
```rust
pub fn handle_status(provider, container, config, global_config) -> VmResult<()>
```

**Private helpers**:
```rust
fn display_status_dashboard(report: &VmStatusReport)
fn display_basic_stopped_status(vm_name: &str, provider_name: &str)
fn has_resource_data(resources: &ResourceUsage) -> bool
fn display_resource_usage(resources: &ResourceUsage)
fn display_service_health(services: &[ServiceStatus])
fn format_memory_mb(mb: u64) -> String
```

**Key workflows**:
1. Get status report from provider
2. Display compact dashboard with emoji status indicators
3. Show resource usage (CPU/memory/disk) with thresholds
4. Show service health for configured services
5. Fallback to basic stopped status if provider doesn't support enhanced status

**Display features**:
- Emoji status indicators (🟢 Running, 🔴 Stopped)
- Resource usage with warning icons (🔥 critical, ⚡ warning, 💚 ok)
- Service health with port mappings
- Context-aware hints (Connect vs Start)

---

### `vm_ops/helpers.rs` (~180 LOC)
**Responsibility**: Shared utilities and service management

**Lines moved**: 691-696, 1471-1507

**Public API**:
```rust
pub fn handle_get_sync_directory(provider: Box<dyn Provider>)
```

**Shared helpers** (pub(super) - internal to vm_ops module):
```rust
pub(super) async fn register_vm_services_helper(vm_name: &str, global_config: &GlobalConfig) -> VmResult<()>
pub(super) async fn unregister_vm_services_helper(vm_name: &str) -> VmResult<()>
```

**Optional additions** (if needed by multiple modules):
- `truncate_string()` - From list.rs
- `format_status()` - From list.rs
- `format_uptime()` - From list.rs
- `format_memory_mb()` - From status.rs

**Responsibilities**:
- VM service lifecycle (register/unregister)
- Utility command implementation (get_sync_directory)
- Optional: formatting helpers used across modules

---

## API Compatibility

### External API: **100% Backward Compatible**

All existing code calling command handlers will continue to work:

```rust
// Before and After - SAME IMPORTS, SAME SIGNATURES
use crate::commands::vm_ops::{
    handle_create, handle_start, handle_stop, handle_destroy,
    handle_ssh, handle_exec, handle_logs, handle_status, handle_list,
};

// All signatures unchanged
handle_create(provider, config, global_config, force, instance, verbose).await?;
handle_start(provider, container, config, global_config).await?;
handle_destroy(provider, container, config, global_config, force).await?;
```

### Internal Changes

**Parent module** (`commands/mod.rs`):
```rust
// Before
pub mod vm_ops;

// After - NO CHANGE
pub mod vm_ops;  // vm_ops is now a directory module

// Usage remains identical
use vm_ops::handle_create;
```

**Import paths unchanged**: Callers still use `commands::vm_ops::handle_*`

---

## Implementation Strategy

### Function Visibility Rules

- **Public** (`pub`): Command handlers called from main CLI dispatcher
- **Super-public** (`pub(super)`): Helpers shared between vm_ops modules (e.g., service registration)
- **Private**: Implementation details within a single module

### Module Dependencies

```
mod.rs (just re-exports)
  │
  ├──> helpers.rs (leaf - no dependencies)
  ├──> list.rs (depends: helpers for formatting - optional)
  ├──> status.rs (no dependencies - self-contained display)
  ├──> lifecycle.rs (depends: helpers for service registration)
  ├──> create.rs (depends: helpers for service registration)
  ├──> interaction.rs (no dependencies)
  └──> destroy.rs (depends: helpers for service unregistration, list for cross-provider queries)
```

**No circular dependencies**: helpers is leaf, destroy depends on list (one-way)

### Testing Strategy

**Current**: All commands tested through one large file

**After**: Each module can be tested independently

```rust
// Test individual command modules
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_with_force_flag() {
        // Test only create module
    }

    #[tokio::test]
    async fn test_stop_unregisters_services() {
        // Test only lifecycle module
    }
}
```

---

## Benefits

### Maintainability
- ✅ Each module has single responsibility (one command or command family)
- ✅ Easier to locate specific command logic (~200 LOC vs 1,507 LOC)
- ✅ Clear ownership per command type

### Testability
- ✅ Can unit test command modules independently
- ✅ Easier to mock service dependencies per command
- ✅ Test isolation prevents unrelated command failures

### Collaboration
- ✅ Fewer merge conflicts (7 files vs 1 large file)
- ✅ Multiple developers can work on different commands simultaneously
- ✅ Clearer code review scope

### Onboarding
- ✅ New contributors can understand one command at a time
- ✅ File names indicate purpose (`create.rs`, `destroy.rs`, etc.)
- ✅ Module documentation focuses on specific command domain

### Performance
- ✅ **No runtime changes** - same compiled code
- ✅ **No API changes** - existing callers unchanged
- ✅ Compiler optimizations identical

---

## Non-Goals

This refactoring **does not**:
- ❌ Change command functionality or fix bugs
- ❌ Modify external API surface (import paths stay the same)
- ❌ Add new commands or features
- ❌ Change error handling patterns
- ❌ Alter message strings or user-facing output
- ❌ Require updates to CLI dispatcher or calling code

---

## Incremental Migration Plan

**Critical**: Do NOT migrate in one PR. Split into mechanically-scoped patches.

### PR #1: Setup + Helpers (~180 LOC)
**Goal**: Establish module structure with leaf module (no dependencies)

**Changes**:
1. Create `vm_ops/` directory
2. Create `vm_ops/mod.rs` with module declarations
3. Create `vm_ops/helpers.rs` (lines 691-696, 1471-1507)
4. Update `vm_ops/mod.rs` to re-export helpers
5. Keep `vm_ops.rs` intact with deprecation comment

**Tests**: Run `cargo test --package vm`

**Git blame**: Preserved for helpers

---

### PR #2: List Module (~200 LOC)
**Goal**: Extract listing logic (self-contained, minimal dependencies)

**Changes**:
1. Create `vm_ops/list.rs` (lines 516-688)
2. Update `mod.rs` to re-export list handlers
3. Optional: Keep format helpers in list.rs or move to helpers.rs

**Dependencies**: Optional dependency on helpers for formatting

**Tests**: Run list command tests

---

### PR #3: Status Module (~250 LOC)
**Goal**: Extract status reporting (self-contained display logic)

**Changes**:
1. Create `vm_ops/status.rs` (lines 1196-1390)
2. Update `mod.rs` to re-export status handler

**Dependencies**: None (self-contained)

**Tests**: Run status command tests

---

### PR #4: Interaction Module (~300 LOC)
**Goal**: Extract SSH/exec/logs commands

**Changes**:
1. Create `vm_ops/interaction.rs` (lines 997-1468)
2. Update `mod.rs` to re-export interaction handlers

**Dependencies**: None (self-contained)

**Tests**: Run SSH/exec/logs tests

---

### PR #5: Lifecycle Module (~280 LOC)
**Goal**: Extract start/stop/restart/provision

**Changes**:
1. Create `vm_ops/lifecycle.rs` (lines 212-513)
2. Update calls to `helpers::register_vm_services_helper()`
3. Update `mod.rs` to re-export lifecycle handlers

**Dependencies**: helpers (already moved in PR #1)

**Tests**: Run lifecycle command tests

---

### PR #6: Create Module (~200 LOC)
**Goal**: Extract VM creation

**Changes**:
1. Create `vm_ops/create.rs` (lines 21-209)
2. Update calls to `helpers::register_vm_services_helper()`
3. Update `mod.rs` to re-export create handler

**Dependencies**: helpers (already moved in PR #1)

**Tests**: Run create command tests

---

### PR #7: Destroy Module (~350 LOC)
**Goal**: Extract destruction logic (single and cross-provider)

**Changes**:
1. Create `vm_ops/destroy.rs` (lines 699-994)
2. Update calls to `helpers::unregister_vm_services_helper()`
3. Update calls to `list::get_all_instances()` for cross-provider queries
4. Update `mod.rs` to re-export destroy handlers

**Dependencies**: helpers, list (both already moved)

**Tests**: Run destroy command tests

---

### PR #8: Cleanup & Delete
**Goal**: Remove old file

**Changes**:
1. Verify ALL tests pass (`cargo test --package vm`)
2. Run `cargo clippy --package vm`
3. Delete `vm_ops.rs`
4. Update CHANGELOG

**Git history**: All blame preserved in new modules

---

### Migration Safety Checklist

**Before each PR**:
- [ ] Create feature branch: `refactor/vm-ops-PR{N}-{module-name}`
- [ ] Run `cargo test --package vm` (must pass)
- [ ] Run `cargo clippy --package vm` (no new warnings)

**During review**:
- [ ] Verify imports use `pub use` re-exports from `mod.rs`
- [ ] Check no circular dependencies between modules
- [ ] Confirm external API unchanged (import paths identical)

**After merge**:
- [ ] Run full integration test suite
- [ ] Check git blame shows original authors
- [ ] Update PR tracking issue

---

## Risks & Mitigation

### Risk: Breaking import paths for external callers
**Mitigation**:
- Use `pub use` re-exports in `vm_ops/mod.rs`
- External callers still import from `commands::vm_ops::handle_*`
- No changes to import paths required

### Risk: Circular dependencies between modules
**Mitigation**:
- Clear dependency hierarchy: helpers → list → destroy
- Use `pub(super)` for internal helpers
- Review dependency graph in each PR

### Risk: Incomplete module separation
**Mitigation**:
- Each module owns complete command logic (no split implementations)
- Helper functions clearly marked as `pub(super)` or private
- Module boundaries follow command domains

### Risk: Lost context from splitting code
**Mitigation**:
- Add module-level documentation (`//!` comments)
- Cross-reference related commands in comments
- Preserve existing function-level documentation

---

## Success Criteria

1. ✅ All existing tests pass without modification
2. ✅ No changes to external API (imports unchanged)
3. ✅ Each module file < 350 LOC
4. ✅ `cargo clippy` passes with no new warnings
5. ✅ Compilation time unchanged or improved
6. ✅ Documentation builds successfully
7. ✅ CLI dispatcher code unchanged (still imports from `vm_ops::*`)

---

## Future Enhancements (Out of Scope)

After this refactor, future improvements become easier:

- **Per-command testing**: Mock providers for individual commands
- **Command-specific error types**: Each module could have domain-specific errors
- **Parallel command execution**: Easier to identify async boundaries
- **Command plugins**: Modular structure supports plugin commands
- **Metrics per command**: Track performance/usage per module

---

## Comparison with Lifecycle Refactor

This proposal is **simpler** than the lifecycle refactor:

| Aspect | Lifecycle Refactor | VM Ops Refactor |
|--------|-------------------|-----------------|
| **Complexity** | High (impl blocks, cross-module helpers) | Low (function-based, clear boundaries) |
| **Dependencies** | Complex (provisioning → packages → helpers) | Simple (mostly independent commands) |
| **Module count** | 9 modules | 7 modules |
| **Largest module** | 600 LOC (provisioning) | 350 LOC (destroy) |
| **API stability** | Must maintain method API via impl blocks | Already function-based, just move |
| **PR count** | 7 PRs | 8 PRs |

**Why simpler**: Commands are already independent functions, not methods on a struct. No need for impl block coordination.

---

## References

- **Current file**: `rust/vm/src/commands/vm_ops.rs` (1,507 LOC)
- **Related modules**: `service_manager.rs` (service registration), `error.rs` (VmResult)
- **External consumers**: `rust/vm/src/main.rs` (CLI dispatcher), integration tests
- **Message definitions**: `vm-messages/src/messages.rs` (MESSAGES constants)

---

## Changelog Entry

```markdown
### Changed
- **Refactored VM operations module**: Split `commands/vm_ops.rs` (1,507 LOC) into 7 focused command modules
  - `vm_ops/create.rs` - VM creation command
  - `vm_ops/lifecycle.rs` - Start/stop/restart/provision commands
  - `vm_ops/destroy.rs` - Destruction (single and cross-provider)
  - `vm_ops/list.rs` - VM listing across providers
  - `vm_ops/status.rs` - Status reporting and display
  - `vm_ops/interaction.rs` - SSH/exec/logs commands
  - `vm_ops/helpers.rs` - Service management and utilities
  - `vm_ops/mod.rs` - Module coordination and re-exports
- **No API changes**: All import paths and function signatures unchanged
- **Improved testability**: Individual command modules can now be tested in isolation
```

---

## Approval

- [ ] Approved by: _______________
- [ ] Date: _______________
- [ ] Implementation branch: `refactor/vm-ops-module-split`
