# Proposal: Fix SSH Command Execution

**Status:** 🔴 Critical Bug
**Priority:** P0 (Blocker)
**Complexity:** Medium
**Estimated Effort:** 1-2 days

---

## Problem Statement

The `vm ssh -c <command>` feature is completely non-functional. When attempting to execute a command inside the VM, the tool incorrectly parses the command as a file path, resulting in "No such file or directory" errors.

### Impact

This bug blocks critical workflows:
- ❌ **Package verification** - Cannot run `vm ssh -c "which ripgrep"` to verify installations
- ❌ **Multi-instance testing** - Cannot run `vm ssh -c "echo 'Project A'"` to verify isolation
- ❌ **File sync validation** - Cannot run `vm ssh -c "ls /path/to/file"` to check file sync
- ❌ **Automated provisioning** - Cannot programmatically interact with VMs in scripts
- ❌ **CI/CD integration** - Cannot execute commands in VMs from automation

**Result:** 3 out of 7 E2E test scenarios are blocked or partially blocked.

### Current Behavior

```bash
$ vm ssh -c "echo hello"
Error: No such file or directory (os error 2)
# The tool treats "echo hello" as a file path instead of a command
```

### Expected Behavior

```bash
$ vm ssh -c "echo hello"
hello

$ vm ssh -c "which ripgrep"
/usr/bin/ripgrep
```

---

## Root Cause Analysis

The SSH command implementation incorrectly parses the `-c` flag argument. Instead of treating it as a command to execute, it's being processed as a file path.

### Likely Code Location

Based on the error, the issue is in:
- `rust/vm/src/commands/ssh.rs` - SSH command implementation
- `rust/vm-cli/src/lib.rs` - CLI argument parsing for `ssh` subcommand

### Investigation Needed

1. Check how the `-c` flag is defined in the CLI parser
2. Verify how the argument is passed to the SSH provider
3. Ensure proper shell escaping/quoting is applied

---

## Proposed Solution

### 1. Fix Argument Parsing

Update the SSH command to properly handle two modes:

**Interactive Mode** (no `-c` flag):
```bash
vm ssh              # Opens interactive shell
vm ssh project-a    # Opens shell in specific VM
```

**Command Execution Mode** (`-c` flag):
```bash
vm ssh -c "command"           # Execute in default VM
vm ssh project-a -c "command" # Execute in specific VM
```

### 2. Implementation Plan

**Step 1: Update CLI Argument Parsing**
```rust
// In rust/vm-cli/src/lib.rs or ssh command definition
#[derive(Parser)]
pub struct SshArgs {
    /// VM name (optional, uses current project if not specified)
    pub vm_name: Option<String>,

    /// Command to execute (if not provided, opens interactive shell)
    #[arg(short = 'c', long = "command")]
    pub command: Option<String>,
}
```

**Step 2: Update SSH Command Handler**
```rust
// In rust/vm/src/commands/ssh.rs
pub fn run(args: SshArgs) -> Result<()> {
    let vm_name = resolve_vm_name(args.vm_name)?;

    match args.command {
        Some(cmd) => {
            // Execute command mode
            execute_command(&vm_name, &cmd)
        }
        None => {
            // Interactive shell mode
            open_interactive_shell(&vm_name)
        }
    }
}

fn execute_command(vm_name: &str, command: &str) -> Result<()> {
    // Build SSH command with proper escaping
    let ssh_args = vec![
        "docker", "exec", "-it", vm_name,
        "/bin/bash", "-c", command
    ];

    // Execute and return exit code
    let status = Command::new(&ssh_args[0])
        .args(&ssh_args[1..])
        .status()?;

    if !status.success() {
        bail!("Command failed with exit code: {}", status.code().unwrap_or(-1));
    }

    Ok(())
}
```

**Step 3: Add Proper Shell Escaping**

Ensure commands with special characters are properly escaped:
```rust
use shell_escape::escape;

fn execute_command(vm_name: &str, command: &str) -> Result<()> {
    let escaped_command = escape(Cow::from(command));
    // Use escaped_command when building docker exec
}
```

### 3. Testing Strategy

**Unit Tests:**
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_ssh_command_mode() {
        let args = SshArgs {
            vm_name: Some("test-vm".into()),
            command: Some("echo hello".into()),
        };
        // Assert proper command construction
    }

    #[test]
    fn test_ssh_interactive_mode() {
        let args = SshArgs {
            vm_name: None,
            command: None,
        };
        // Assert interactive mode is triggered
    }

    #[test]
    fn test_command_escaping() {
        let cmd = "echo 'hello world' && ls -la";
        // Assert proper escaping of special characters
    }
}
```

**Integration Tests:**
```rust
#[test]
fn test_ssh_command_execution() -> Result<()> {
    let temp_dir = create_temp_project()?;

    // Create VM
    run_vm_command(&["create"], &temp_dir)?;
    run_vm_command(&["start"], &temp_dir)?;

    // Test command execution
    let output = run_vm_command(&["ssh", "-c", "echo hello"], &temp_dir)?;
    assert_eq!(output.trim(), "hello");

    // Test with complex command
    let output = run_vm_command(&["ssh", "-c", "echo 'a b c' | wc -w"], &temp_dir)?;
    assert_eq!(output.trim(), "3");

    cleanup_vm(&temp_dir)?;
    Ok(())
}
```

---

## Edge Cases to Handle

1. **Special characters in commands:**
   - Single quotes: `vm ssh -c "echo 'hello'"`
   - Double quotes: `vm ssh -c 'echo "hello"'`
   - Pipes: `vm ssh -c "ls | grep foo"`
   - Redirects: `vm ssh -c "echo hello > /tmp/test"`

2. **Exit codes:**
   - Preserve command exit code for scripting
   - Return non-zero on command failure

3. **Output handling:**
   - Stream stdout/stderr in real-time
   - Don't buffer large outputs
   - Preserve colors/formatting when in TTY

4. **Environment variables:**
   - `vm ssh -c "echo $HOME"` should expand inside VM, not host

---

## Acceptance Criteria

- [ ] `vm ssh -c "command"` executes command in VM and returns output
- [ ] `vm ssh` (no `-c`) opens interactive shell (existing behavior preserved)
- [ ] Command exit codes are properly propagated
- [ ] Special characters (quotes, pipes, redirects) work correctly
- [ ] Works with both default VM and named VMs (`vm ssh project-a -c "cmd"`)
- [ ] Unit tests added for argument parsing
- [ ] Integration tests added for command execution
- [ ] E2E test scenarios unblocked (Package Manager, Multi-Instance, File Sync)

---

## Alternative Approaches Considered

### Option A: Use `--exec` instead of `-c`
```bash
vm ssh --exec "command"
```
**Rejected:** `-c` is standard SSH convention and already documented

### Option B: Positional argument
```bash
vm ssh "command"
```
**Rejected:** Conflicts with VM name argument, ambiguous

---

## Dependencies

- None (self-contained fix)

---

## Migration Plan

This is a bug fix, not a breaking change. No migration needed.

---

## Documentation Updates

Update `vm ssh --help`:
```
Execute commands or open interactive shell in VM

Usage:
  vm ssh [VM_NAME]              Open interactive shell
  vm ssh -c "command"           Execute command in default VM
  vm ssh [VM_NAME] -c "command" Execute command in specific VM

Options:
  -c, --command <COMMAND>  Command to execute (non-interactive)
  -h, --help               Print help

Examples:
  vm ssh                        # Interactive shell
  vm ssh -c "ls -la"            # Execute command
  vm ssh -c "which ripgrep"     # Check if package installed
  vm ssh project-a -c "pwd"     # Execute in specific VM
```

---

## Timeline

- **Day 1:** Fix argument parsing, update command handler, add unit tests
- **Day 2:** Add integration tests, update documentation, validate E2E scenarios

---

## Success Metrics

- ✅ All 3 blocked E2E test scenarios now pass
- ✅ CI/CD workflows can programmatically interact with VMs
- ✅ No regression in interactive SSH mode
- ✅ Zero "No such file or directory" errors when using `-c`
