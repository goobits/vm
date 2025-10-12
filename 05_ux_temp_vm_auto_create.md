# Proposal: Auto-Create Temp VM on SSH/Mount Commands

**Status:** Draft
**Priority:** Medium (UX Enhancement)
**Effort:** 2-3 hours
**Impact:** Improved user experience for temporary VM workflow

---

## Problem Statement

Currently, when users try to interact with a temp VM that doesn't exist, they receive an abrupt error message:

```bash
miko@Mac goobits-forms % vm temp mount .
Error: Internal error: No temp VM found. Create one first with: vm temp create
```

This forces users to:
1. See the error
2. Manually run `vm temp create`
3. Re-run their original command

**Comparison with Regular VM Workflow:**

The regular `vm ssh` command has a much better UX when the VM isn't running (from `interaction.rs:79-125`):
```bash
$ vm ssh
VM 'my-project' is not running.

Would you like to start it now? [Y/n]:
```

If user presses Enter or types "y", the VM starts automatically and SSH connects.

---

## Proposed Solution

Add **auto-creation prompts** to temp VM commands that require an existing VM, matching the UX pattern from the regular SSH workflow.

### Affected Commands

1. **`vm temp ssh`** - SSH into temp VM
2. **`vm temp mount <path>`** - Add mount to temp VM
3. **`vm temp unmount`** - Remove mount from temp VM
4. **`vm temp start`** - Start temp VM
5. **`vm temp stop`** - Stop temp VM
6. **`vm temp restart`** - Restart temp VM

**Not affected:**
- `vm temp create` - Already creates VM
- `vm temp status` - Should show helpful message, not auto-create
- `vm temp list` - Should show empty state, not auto-create
- `vm temp mounts` - Should show helpful message, not auto-create
- `vm temp destroy` - Can't destroy non-existent VM

---

## User Experience Design

### Scenario 1: `vm temp ssh` (No VM Exists)

**Current behavior:**
```bash
$ vm temp ssh
Error: Internal error: No temporary VM found. Create one first with: vm temp create
```

**Proposed behavior:**
```bash
$ vm temp ssh
No temporary VM found.

Would you like to create one now? [Y/n]: █
```

**If user presses Enter or types "y":**
```bash
Would you like to create one now? [Y/n]: y

🚀 Creating temporary VM...
✓ Container started
✓ No mounts configured

💡 Tip: Add mounts with 'vm temp mount <path>'

Connecting to temporary VM...
developer@vm-temp-dev:~$ █
```

**If user types "n":**
```bash
Would you like to create one now? [Y/n]: n
Cancelled. Create a temp VM with: vm temp create <directory>
```

### Scenario 2: `vm temp mount .` (No VM Exists)

**Current behavior:**
```bash
$ vm temp mount .
Error: Internal error: No temp VM found. Create one first with: vm temp create
```

**Proposed behavior:**
```bash
$ vm temp mount .
No temporary VM found.

Would you like to create one with this mount? [Y/n]: █
```

**If user presses Enter or types "y":**
```bash
Would you like to create one with this mount? [Y/n]: y

🚀 Creating temporary VM...
✓ Container started
🔗 Mount added: /Users/miko/goobits-forms (rw)

💡 Tip: Connect with 'vm temp ssh'
```

### Scenario 3: Non-Interactive Environments

When not in an interactive terminal (CI/CD, scripts, etc.), fail with clear error:
```bash
$ vm temp ssh < /dev/null
Error: No temporary VM found. Create one first with: vm temp create

Note: In non-interactive mode, VM must be created explicitly.
```

---

## Implementation Details

### Files to Modify

#### 1. `/workspace/rust/vm-temp/src/temp_ops.rs`

**Changes needed:**

##### A. Extract Auto-Create Helper (New Function)

Add after line 685:
```rust
/// Helper function to prompt for temp VM creation
/// Returns true if user wants to create, false otherwise
fn prompt_for_temp_vm_creation(action_context: &str) -> bool {
    use std::io::{self, Write};

    // Check if we're in an interactive terminal
    if !io::stdin().is_terminal() {
        return false;
    }

    println!("No temporary VM found.\n");
    print!("Would you like to create one {}? [Y/n]: ", action_context);

    // If stdout flush fails, continue anyway
    let _ = io::stdout().flush();

    let mut input = String::new();
    match io::stdin().read_line(&mut input) {
        Ok(_) => {
            let input = input.trim().to_lowercase();
            // Default to 'yes' on empty input (just pressing Enter)
            input.is_empty() || input == "y" || input == "yes"
        }
        Err(_) => false,
    }
}
```

##### B. Update `ssh()` Method (Lines 117-132)

**Before:**
```rust
pub fn ssh(provider: Box<dyn Provider>) -> Result<()> {
    let state_manager = StateManager::new().map_err(|e| {
        VmError::Internal(format!(
            "Failed to initialize state manager for SSH connection: {}",
            e
        ))
    })?;

    if !state_manager.state_exists() {
        return Err(VmError::Internal(
            "No temporary VM found. Create one first with: vm temp create".to_string(),
        ));
    }

    provider.ssh(None, &PathBuf::from("."))
}
```

**After:**
```rust
pub fn ssh(provider: Box<dyn Provider>, config: VmConfig) -> Result<()> {
    let state_manager = StateManager::new().map_err(|e| {
        VmError::Internal(format!(
            "Failed to initialize state manager for SSH connection: {}",
            e
        ))
    })?;

    if !state_manager.state_exists() {
        // Prompt user to create temp VM
        if prompt_for_temp_vm_creation("now") {
            info!("\n🚀 Creating temporary VM...");

            // Create temp VM with current directory as mount
            let project_dir = std::env::current_dir().map_err(|e| {
                VmError::Filesystem(format!("Failed to get current directory: {}", e))
            })?;

            let mounts = vec![project_dir.display().to_string()];
            Self::create(mounts, false, config, provider.clone())?;

            info!("Connecting to temporary VM...");
            // Fall through to SSH connection below
        } else {
            info!("Cancelled. Create a temp VM with: vm temp create <directory>");
            return Ok(());
        }
    }

    provider.ssh(None, &PathBuf::from("."))
}
```

##### C. Update `mount()` Method (Lines 219-324)

Add after line 227 (right after state existence check):

```rust
if !state_manager.state_exists() {
    // Prompt user to create temp VM with this mount
    if prompt_for_temp_vm_creation("with this mount") {
        info!("\n🚀 Creating temporary VM...");

        // Create temp VM with the requested mount
        Self::create(vec![path.clone()], false, _config, provider.clone())?;

        info!("💡 Tip: Connect with 'vm temp ssh'");
        return Ok(());
    } else {
        info!("Cancelled. Create a temp VM with: vm temp create <directory>");
        return Ok(());
    }
}
```

##### D. Update Other Commands (start, stop, restart, unmount)

For commands that **require** an existing VM but shouldn't auto-create, keep the error but make it friendlier:

**Lines 571-575 (stop), 602-606 (start), 649-652 (restart):**

Replace:
```rust
if !state_manager.state_exists() {
    return Err(VmError::Internal(
        "No temp VM found. Create one with: vm temp create <directory>".to_string(),
    ));
}
```

With:
```rust
if !state_manager.state_exists() {
    info!("No temporary VM found.");
    info!("💡 Create one with: vm temp create <directory>");
    info!("   Or use 'vm temp ssh' to create and connect automatically");
    return Err(VmError::NotFound(
        "No temporary VM exists".to_string(),
    ));
}
```

#### 2. `/workspace/rust/vm/src/commands/temp.rs`

**Update SSH handler signature** (Line 24):

**Before:**
```rust
TempSubcommand::Ssh => TempVmOps::ssh(provider),
```

**After:**
```rust
TempSubcommand::Ssh => TempVmOps::ssh(provider, config),
```

**Update Mount handler** (Line 27):

**Before:**
```rust
TempSubcommand::Mount { path, yes } => TempVmOps::mount(path.clone(), *yes, provider),
```

**After:**
```rust
TempSubcommand::Mount { path, yes } => TempVmOps::mount(path.clone(), *yes, provider, config),
```

#### 3. `/workspace/rust/vm-temp/src/lib.rs`

Update the public API exports to include the new signature with `VmConfig` parameter.

---

## Implementation Checklist

### Phase 1: Core Auto-Create for SSH (1 hour)
- [ ] Add `prompt_for_temp_vm_creation()` helper function
- [ ] Update `TempVmOps::ssh()` to accept `VmConfig` parameter
- [ ] Add auto-create logic to `ssh()` method
- [ ] Update `handle_temp_command()` to pass config to ssh
- [ ] Test interactive prompt (Y/n/Enter default)
- [ ] Test non-interactive mode (script/CI)

### Phase 2: Auto-Create for Mount (30 minutes)
- [ ] Update `TempVmOps::mount()` to accept `VmConfig` parameter
- [ ] Add auto-create logic to `mount()` method
- [ ] Update `handle_temp_command()` to pass config to mount
- [ ] Test mount with non-existent VM
- [ ] Verify mount is applied after creation

### Phase 3: Improve Error Messages (30 minutes)
- [ ] Update `start()`, `stop()`, `restart()`, `unmount()` error messages
- [ ] Add helpful hints pointing to auto-create commands
- [ ] Make errors use `VmError::NotFound` instead of `Internal`
- [ ] Test all error paths

### Phase 4: Documentation (30 minutes)
- [ ] Update `vm temp --help` with auto-create behavior
- [ ] Add examples to user guide
- [ ] Update CHANGELOG.md

---

## Testing Plan

### Unit Tests

Add to `/workspace/rust/vm-temp/tests/`:

```rust
#[test]
fn test_ssh_prompts_for_creation_when_no_vm() {
    // Setup: No existing temp VM state
    // Action: Call ssh() in non-interactive mode
    // Assert: Returns error with helpful message
}

#[test]
fn test_mount_prompts_for_creation_when_no_vm() {
    // Setup: No existing temp VM state
    // Action: Call mount() in non-interactive mode
    // Assert: Returns error with helpful message
}
```

### Integration Tests

```bash
# Test 1: SSH auto-create (interactive simulation)
$ echo "y" | vm temp ssh
# Expected: VM created, SSH connected

# Test 2: SSH auto-create declined
$ echo "n" | vm temp ssh
# Expected: Cancelled message, no VM created

# Test 3: Mount auto-create (interactive simulation)
$ echo "y" | vm temp mount .
# Expected: VM created with mount

# Test 4: Non-interactive mode
$ vm temp ssh < /dev/null
# Expected: Error with clear message

# Test 5: Existing VM (no prompt)
$ vm temp create .
$ vm temp ssh
# Expected: Direct SSH connection, no prompt
```

### Manual Testing

- [ ] Test on macOS terminal (interactive)
- [ ] Test on Linux terminal (interactive)
- [ ] Test in CI environment (non-interactive)
- [ ] Test in VS Code integrated terminal
- [ ] Test with `--yes` flag for mount
- [ ] Test cancellation with Ctrl+C during prompt

---

## Alternative Designs Considered

### Option 1: Always Auto-Create (No Prompt)

**Rejected because:**
- Too magic - users might not realize what's happening
- Could create unintended VMs in scripts
- No control over mount configuration

### Option 2: Add `--create` Flag

```bash
vm temp ssh --create
vm temp mount . --create
```

**Rejected because:**
- Extra typing for common case
- Not discoverable (user has to know flag exists)
- Doesn't follow existing VM workflow pattern

### Option 3: Separate Command

```bash
vm temp create-and-ssh
vm temp create-and-mount .
```

**Rejected because:**
- Command proliferation
- Not intuitive
- Doesn't match regular VM pattern

---

## Benefits

1. **Improved UX** - Matches the pattern from regular `vm ssh` command
2. **Fewer Steps** - Reduces 3-step process to 1 step + confirmation
3. **Better Discovery** - Users learn about temp VM workflow naturally
4. **Consistent** - Follows existing prompt pattern in the codebase
5. **Safe** - Interactive prompt prevents accidental creation
6. **Script-Friendly** - Non-interactive mode still requires explicit creation

---

## Backwards Compatibility

✅ **Fully backwards compatible**

- Existing scripts that handle errors will continue to work
- Non-interactive environments see no behavior change
- Users who always create VMs explicitly won't notice difference
- Only affects interactive terminal sessions

---

## Future Enhancements

### Phase 5 (Optional): Smart Mount Detection

When creating from SSH, auto-detect common directories:
```bash
$ cd ~/my-project && vm temp ssh
No temporary VM found.

Would you like to create one with mount /Users/miko/my-project? [Y/n]:
```

### Phase 6 (Optional): Remember Last Configuration

```bash
$ vm temp ssh
No temporary VM found.

Last temp VM had 3 mounts. Recreate with same configuration? [Y/n]:
```

### Phase 7 (Optional): Add `--auto` Flag

For users who always want auto-create:
```bash
vm temp ssh --auto  # Never prompt, always create if missing
```

Could be set in global config:
```yaml
temp_vm:
  auto_create: true  # Default: false
```

---

## Success Criteria

- [ ] `vm temp ssh` prompts to create VM when none exists
- [ ] `vm temp mount` prompts to create VM when none exists
- [ ] Prompt defaults to "yes" on Enter key
- [ ] Non-interactive mode fails with clear error
- [ ] Existing VMs connect without prompt
- [ ] All tests pass
- [ ] Documentation updated
- [ ] User feedback is positive (fewer "how do I" questions)

---

## Estimated Timeline

- **Phase 1 (SSH auto-create):** 1 hour
- **Phase 2 (Mount auto-create):** 30 minutes
- **Phase 3 (Error improvements):** 30 minutes
- **Phase 4 (Documentation):** 30 minutes
- **Testing:** 30 minutes

**Total:** 2.5-3 hours

---

## Related Issues

- Similar pattern already exists in `vm ssh` (interaction.rs:79-125)
- Temp VM workflow documented in user guides
- StateManager handles persistence correctly

---

## Notes

This proposal maintains the philosophy that:
- **Interactive users** get helpful prompts and shortcuts
- **Non-interactive scripts** require explicit commands for safety
- **Error messages** guide users to correct actions

The implementation reuses existing patterns from the regular VM workflow, ensuring consistency across the codebase.
