# VM Messages Migration Checklist

## Overview
This proposal tracks the migration of all user-facing messages to use the centralized `vm-messages` crate and MESSAGES constant.

**Current Status:** ~15% adoption (6 of 40 files properly using MESSAGES)
**Target:** 100% adoption for consistent, maintainable user communication

---

## Migration Pattern Guide

### What Has Worked Well

Based on successfully migrated files (`instance.rs`, `config_ops.rs`), here's the proven pattern:

#### 1. Import Setup
```rust
use vm_cli::msg;                      // Template substitution macro (note: from vm-cli, not vm-messages)
use vm_core::{vm_println, vm_error};  // Output macros
use vm_messages::messages::MESSAGES;  // Message constants
```

**Why three imports?** The architecture separates concerns:
- `vm-messages` = Static message templates only (no dependencies)
- `vm-cli` = Runtime template substitution (`msg!` macro)
- `vm-core` = Output formatting (`vm_println!`, `vm_error!` macros)

#### 2. Simple Messages (No Variables)
**Before:**
```rust
println!("No changes made");
```
**After:**
```rust
vm_println!("{}", MESSAGES.config_no_changes);
```

#### 3. Messages with Variables
**Before:**
```rust
println!("Set {} = {} in {}", field, value, path);
```
**After:**
```rust
vm_println!("{}", msg!(
    MESSAGES.config_set_success,
    field = field,
    value = value,
    path = path
));
```

#### 4. Error Messages
**Before:**
```rust
eprintln!("VM '{}' is ambiguous", name);
for item in items {
    eprintln!("  • {}", item);
}
```
**After:**
```rust
vm_error!("{}", MESSAGES.vm_ambiguous);
for item in items {
    vm_error!("  • {}", item);
}
```

#### 5. Multi-line with Format Preservation
**Before:**
```rust
println!("Would you like to start it now? (y/N): ");
```
**After:**
```rust
vm_println!("{}", msg!(MESSAGES.vm_start_prompt, name = vm_name));
```

### Key Patterns That Work

✅ **Use `msg!()` for variable substitution**
- Template format in MESSAGES: `"Set {field} = {value}"`
- Usage: `msg!(MESSAGES.template, field = "x", value = "y")`

✅ **Always wrap MESSAGES in formatting macro**
- Correct: `vm_println!("{}", MESSAGES.success)`
- Wrong: `vm_println!(MESSAGES.success)` ← won't compile

**Why `"{}"`?** The vm_println! macro expects a format string as its first argument. MESSAGES constants are `&'static str`, not format strings. The `"{}"` tells the formatter to insert the string value.

✅ **Preserve emoji and formatting**
- Messages can include emojis: `"🚀 Creating '{name}'..."`
- Multi-line messages use `\n` in template string

✅ **Keep logic separate from messages**
- Good: Conditional logic chooses which MESSAGES constant to use
- Bad: Putting conditionals inside message templates

✅ **Type compatibility with `msg!()`**
- Any type implementing `Into<String>` or `ToString` works
- Examples: `&str`, `String`, `PathBuf`, `usize`, custom types with Display
- The macro calls `.to_string()` automatically on each variable

### Adding New Messages

When you need a message that doesn't exist:

**Naming Convention:** `{category}_{action}` or `{category}_{state}`
- Category: `vm`, `config`, `temp_vm`, `docker`, `provider`, `pkg`, etc.
- Action/State: `creating`, `created`, `destroyed`, `success`, `failed`, `hint`, etc.
- Examples: `vm_creating`, `config_set_success`, `docker_not_running`

1. **Add to `vm-messages/src/messages.rs`:**
```rust
pub struct Messages {
    // ... existing fields
    pub vm_destroy_confirm: &'static str,       // Category: vm, Action: destroy (confirm)
}

pub const MESSAGES: Messages = Messages {
    // ... existing values
    vm_destroy_confirm: "Confirm destruction? (y/N): ",
};
```

2. **Use in your code:**
```rust
vm_println!("{}", msg!(MESSAGES.vm_destroy_confirm, name = vm_name));
```

### Common Pitfalls to Avoid

❌ **Don't mix raw println! with MESSAGES**
```rust
// Bad - inconsistent
println!("Starting VM...");
vm_println!("{}", MESSAGES.vm_created);
```

❌ **Don't put too much logic in templates**
```rust
// Bad - hard to maintain
pub vm_status: "Status: {is_running ? '🟢 Running' : '🔴 Stopped'}"

// Good - use code to select message
pub vm_running: "Status: 🟢 Running"
pub vm_stopped: "Status: 🔴 Stopped"
```

❌ **Don't forget the wrapping `{}`**
```rust
// Wrong - won't compile
vm_println!(MESSAGES.success)

// Correct
vm_println!("{}", MESSAGES.success)
```

### Real-World Examples

**From `instance.rs` (lines 96-101):**
```rust
vm_error!("{}", MESSAGES.vm_ambiguous);
for name in &matches {
    vm_error!("  • {}", name);
}
vm_error!("{}", msg!(MESSAGES.vm_using, name = &matches[0]));
```

**From `config_ops.rs` (lines 239-248):**
```rust
vm_println!("{}", msg!(
    MESSAGES.config_set_success,
    field = field,
    value = value,
    path = config_path.display().to_string()
));
vm_println!("{}", MESSAGES.config_apply_changes_hint);
```

---

## Migration Status Legend
- ✅ **Complete** - All messages use MESSAGES constant
- 🔄 **In Progress** - Partial migration (some MESSAGES, some raw println!)
- ❌ **Not Started** - Only raw println!/eprintln! or vm_macros with inline strings

---

## High Priority Files (400+ message statements)

### vm/src/commands/ (Core CLI Commands)

- [ ] ❌ **vm_ops.rs** (180 statements)
  - VM lifecycle: create, start, stop, restart, provision, destroy, list, status, ssh, exec, logs
  - Most critical user-facing file in the codebase

- [ ] ❌ **plugin.rs** (67 statements)
  - Plugin management: list, install, uninstall, enable, disable, update

- [ ] ❌ **pkg.rs** (43 statements)
  - Package registry: install, uninstall, list, search, publish

- [ ] 🔄 **uninstall.rs** (27 statements + 31 vm_macros)
  - Uninstall command and cleanup

- [ ] ❌ **config.rs** (25 statements + 10 vm_macros)
  - Config CLI: get, set, list, unset, preset commands

- [ ] ❌ **doctor.rs** (19 statements)
  - Health check diagnostics

- [ ] ❌ **mod.rs** (16 statements)
  - Command module initialization

- [ ] ❌ **auth.rs** (15 statements)
  - Auth proxy commands

- [ ] ❌ **plugin_new.rs** (12 statements)
  - Plugin scaffolding

- [ ] ❌ **update.rs** (10 statements)
  - Self-update command

### vm-config/src/ (Configuration Management)

- [ ] 🔄 **cli/commands/init.rs** (29 statements + 6 MESSAGES)
  - Project initialization wizard

- [ ] 🔄 **config_ops.rs** (27 statements + 12 MESSAGES)
  - Core config operations

### vm-provider/src/ (Provider Operations)

- [ ] ❌ **docker/lifecycle.rs** (26 statements + 15 vm_macros)
  - Docker container lifecycle

- [ ] 🔄 **progress.rs** (22 statements + 16 MESSAGES)
  - Progress reporting

---

## Medium Priority Files (100-200 message statements)

### vm-config/src/

- [ ] 🔄 **ports/registry.rs** (21 statements + 3 MESSAGES)
  - Port allocation and registry

- [ ] ❌ **cli/formatting.rs** (18 statements)
  - CLI output formatting

- [ ] ❌ **detector/mod.rs** (15 statements)
  - Framework detection

- [ ] ❌ **validate.rs** (13 statements)
  - Configuration validation

- [ ] ❌ **ports/validator.rs** (10 statements)
  - Port validation

- [ ] ❌ **cli/commands/config.rs** (9 statements + 6 vm_macros)
  - Config subcommands

- [ ] ❌ **cli/commands/preset.rs** (8 statements + 4 vm_macros)
  - Preset management

- [ ] ❌ **presets.rs** (7 statements)
  - Preset definitions

- [ ] ❌ **detector/frameworks/mod.rs** (6 statements)
  - Framework detector module

- [ ] ❌ **ports/allocator.rs** (5 statements)
  - Port allocator

- [ ] ❌ **detector/languages/mod.rs** (4 statements)
  - Language detection

### vm-provider/src/

- [ ] 🔄 **tart/provider.rs** (16 statements + 15 MESSAGES)
  - Tart VM provider

- [ ] ❌ **docker/build.rs** (8 statements + 10 vm_macros)
  - Docker image building

- [ ] ❌ **docker/compose.rs** (7 statements + 5 vm_macros)
  - Docker Compose integration

- [ ] ❌ **docker/command.rs** (6 statements + 8 vm_macros)
  - Docker command execution

- [ ] ❌ **vagrant/provider.rs** (5 statements)
  - Vagrant provider

- [ ] ❌ **audio.rs** (3 statements + 2 vm_macros)
  - Audio device setup

- [ ] ✅ **common/instance.rs** (2 MESSAGES, 0 raw println!)
  - Instance info (already migrated)

### vm/src/

- [ ] ❌ **main.rs** (10 statements + 8 vm_macros)
  - CLI entry point

- [ ] ❌ **cli.rs** (5 statements)
  - CLI argument parsing

---

## Low Priority Files (System/Infrastructure)

### vm-core/src/

- [ ] ❌ **lib.rs** (8 statements + 6 vm_macros)
  - Core utilities

- [ ] ❌ **error.rs** (7 statements + 4 vm_macros)
  - Error handling

- [ ] ❌ **macros.rs** (3 statements + 2 vm_macros)
  - Macro definitions

- [ ] ❌ **validation.rs** (2 statements)
  - Input validation

### vm-provider/src/docker/

- [ ] ❌ **host_packages.rs** (5 statements)
  - Host package detection

- [ ] ❌ **mod.rs** (4 statements + 5 vm_macros)
  - Docker provider module

### vm-config/src/ports/

- [ ] ❌ **conflict.rs** (4 statements)
  - Port conflict detection

- [ ] ❌ **range.rs** (3 statements)
  - Port range management

### Other

- [ ] ❌ **vm-installer/src/lib.rs** (12 statements)
  - Installation logic

- [ ] ❌ **vm-temp/src/lib.rs** (Unknown - needs audit)
  - Temporary VM management

---

## Statistics

### By Status
- ✅ Complete: 1 file (2%)
- 🔄 In Progress: 5 files (13%)
- ❌ Not Started: 34 files (85%)

### By Priority
- High Priority: 13 files (~400 statements)
- Medium Priority: 22 files (~180 statements)
- Low Priority: 5 files (~40 statements)

### Total Migration Scope
- **~620 message statements** need conversion
- **40 files** need updating
- **3 crates** primarily affected: vm, vm-config, vm-provider

---

## Testing Strategy

### Updating Tests After Migration

**Challenge:** Many tests assert on message output. After migration, these assertions need updating.

**Before Migration:**
```rust
#[test]
fn test_vm_create() {
    let output = run_command("vm create");
    assert!(output.contains("Creating 'myproject'"));
    assert!(output.contains("Created successfully"));
}
```

**After Migration:**
```rust
#[test]
fn test_vm_create() {
    let output = run_command("vm create");
    // Option 1: Assert on MESSAGES constants (preferred - less brittle)
    assert!(output.contains(&msg!(MESSAGES.vm_creating, name = "myproject")));
    assert!(output.contains(MESSAGES.vm_created_success));

    // Option 2: Keep existing strings (if testing user experience, not implementation)
    assert!(output.contains("Creating 'myproject'"));
}
```

**Test Migration Checklist:**
1. Identify all tests using `assert!(...contains("message"))` patterns
2. Decide: Test implementation (use MESSAGES) or UX (keep strings)?
3. Update assertions to match new message format
4. Run `cargo test --workspace` to catch all breaks

**Recommended Approach:**
- Integration tests: Keep string assertions (test user experience)
- Unit tests: Use MESSAGES constants (test implementation)

---

## Phase 1 Execution Plan

### Target: vm_ops.rs (180 statements)

**Approach:** Function-by-function migration to minimize risk

**Order of Operations:**
1. Add required imports (vm_cli::msg, MESSAGES)
2. Migrate one function at a time: `handle_destroy` first (currently being worked on)
3. Run `cargo test --package vm` after each function
4. Commit after each successful function migration

**Rollback Strategy:**
- If Phase 1 reveals systemic issues (e.g., msg!() performance problems):
  1. Keep completed migrations in feature branch
  2. Document issues in this proposal
  3. Pause migration until issues resolved
  4. Resume from last successful function

### Phase Rollout

1. **Phase 1:** Migrate vm_ops.rs (highest priority, 180 statements)
2. **Phase 2:** Migrate remaining vm/src/commands/ files
3. **Phase 3:** Complete partial migrations (files with 🔄)
4. **Phase 4:** Migrate vm-config and vm-provider files
5. **Phase 5:** Audit and migrate vm-temp, vm-installer

---

## Success Criteria

### Per-Phase Success (Measured after each phase):
- [ ] Zero raw println!/eprintln! in migrated files
- [ ] All tests pass (`cargo test --workspace`)
- [ ] No user-reported message formatting regressions
- [ ] Build time impact < 5% (measured with `cargo build --timings`)

### Overall Success (Measured at project completion):
- [ ] All user-facing messages use MESSAGES constant
- [ ] Consistent message formatting across all commands
- [ ] All messages are localization-ready (centralized templates)
- [ ] Test suite maintains coverage with updated assertions
