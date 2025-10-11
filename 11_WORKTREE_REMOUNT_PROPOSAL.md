# Proposal 11: Worktree Remount Support

## Status
🟡 PROPOSED

## Problem Statement

When using Git worktrees, developers may create new worktrees **after** their VM has been created. Currently, these new worktrees are not accessible inside the VM because Docker volume mounts are configured only at container creation time (`vm create`). There is no way to refresh the mounts without destroying and recreating the entire VM.

### Current Limitation

```bash
# Day 1: Create VM
vm create

# Day 2: Create new worktree
git worktree add ../feature-branch

# Try to access it in VM
vm ssh
> cd /worktrees/feature-branch  # ❌ Does not exist!
```

The worktree directory `/worktrees/feature-branch` is not mounted because the container was created before the worktree existed.

## Proposed Solution

`vm ssh` automatically detects new worktrees and offers to refresh mounts with safety checks to avoid disrupting active work.

### User Experience

**Scenario 1: Safe to refresh (no other SSH sessions)**
```bash
git worktree add ../feature-branch
vm ssh

⚠️  New worktree detected: feature-branch
   Refresh mounts now? (takes 2s) (Y/n): y

⏳ Refreshing mounts...
✓ Stopped container (1s)
✓ Updated volumes (0.5s)
✓ Started container (1s)
🔗 Connected to VM
```

**Scenario 2: Not safe (other sessions active)**
```bash
vm ssh

⚠️  New worktrees detected but can't refresh:
   2 other SSH sessions are active

💡 Close other sessions or use:
   vm ssh --force-refresh  (will disconnect others)
   vm ssh --no-refresh     (connect without refresh)

🔗 Connecting without refresh...
```

**Scenario 3: Force refresh**
```bash
vm ssh --force-refresh

⚠️  Warning: This will disconnect 2 active SSH sessions
Continue? (y/N): y

⏳ Refreshing mounts...
🔗 Connected to VM
```

**Scenario 4: Skip auto-detection**
```bash
vm ssh --no-refresh

🔗 Connected to VM
```

## Technical Design

### Algorithm

```rust
pub fn handle_ssh(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
    flags: SshFlags,
) -> Result<()> {
    // 1. Skip detection if user specified --no-refresh
    if flags.no_refresh {
        return connect_ssh(provider, container, config);
    }

    // 2. Detect all current worktrees from Git
    let worktrees = detect_worktrees(&config)?;
    let current_mounts = get_container_mounts(provider, container)?;

    // 3. Check if refresh needed
    if worktrees_match(&worktrees, &current_mounts) {
        // No refresh needed - instant connection
        return connect_ssh(provider, container, config);
    }

    // 4. Refresh needed - check if safe
    let active_sessions = count_active_ssh_sessions(container)?;

    if active_sessions > 0 && !flags.force_refresh {
        // Not safe - show warning and connect without refresh
        show_refresh_warning(active_sessions);
        return connect_ssh(provider, container, config);
    }

    // 5. Safe to refresh (or force flag used)
    if flags.force_refresh && active_sessions > 0 {
        if !confirm_disconnect(active_sessions)? {
            return connect_ssh(provider, container, config);
        }
    } else {
        // Prompt user
        if !prompt_refresh(&worktrees)? {
            return connect_ssh(provider, container, config);
        }
    }

    // 6. Perform refresh
    println!("⏳ Refreshing mounts...");
    provider.stop(container)?;
    update_compose_volumes(container, &worktrees)?;
    provider.start(container)?;

    // 7. Track this session and connect
    increment_ssh_session_count(container)?;
    let result = connect_ssh(provider, container, config);
    decrement_ssh_session_count(container)?;

    result
}
```

### Key Components

1. **Worktree Detection** (reuse existing `vm-config` logic)
   - Already implemented in `vm-config/src/detector.rs`
   - Parses `.git/worktrees/` directory
   - Returns list of worktree paths

2. **Mount Inspection** (new)
   - Use `docker inspect` to get current container mounts
   - Parse JSON output for volume bindings
   - Compare with expected worktree mounts

3. **Session Tracking** (new)
   - Store active SSH session count in `~/.vm/state/vm-project.json`
   - Increment on SSH connect, decrement on disconnect
   - Use for safety checks before restart

4. **Safety Detection** (new)
   - Check state file for active SSH sessions
   - Double-check with PTY count via `ls /dev/pts/`
   - Block refresh if other users connected (unless `--force-refresh`)

5. **Volume Update** (existing)
   - Modify `docker-compose.yml` to add new volumes
   - Use existing `vm-provider/src/docker/compose.rs` logic

6. **Fast Restart** (existing)
   - Use provider's `stop()` and `start()` methods
   - No data loss - container state preserved
   - Takes ~2-3 seconds total

### CLI Changes

**File**: `rust/vm/src/cli/mod.rs`

```rust
Command::Ssh {
    container: Option<String>,
    path: Option<PathBuf>,
    command: Option<String>,

    /// Force refresh mounts (disconnects other sessions)
    #[arg(long)]
    force_refresh: bool,  // ← NEW FLAG

    /// Skip automatic mount refresh detection
    #[arg(long)]
    no_refresh: bool,  // ← NEW FLAG
},
```

### Command Handler Changes

**File**: `rust/vm/src/commands/vm_ops/interaction.rs`

- Modify existing `handle_ssh()` to include auto-detection logic
- Add `count_active_ssh_sessions()` helper
- Add `prompt_refresh()` for user confirmation
- Add session tracking (increment/decrement)

**New File**: `rust/vm/src/state.rs`

- Implement `VmState` struct with `active_ssh_sessions` field
- Load/save state to `~/.vm/state/{project-name}.json`
- Thread-safe increment/decrement operations

### Provider Interface (No Changes)

The existing provider interface already supports all needed operations:
- `provider.stop()` - Stop container
- `provider.start()` - Start container
- Docker-specific: `docker inspect` for mount inspection

## Implementation Tasks

### Core Functionality
- [ ] Add `--force-refresh` and `--no-refresh` flags to CLI
- [ ] Create `rust/vm/src/state.rs` for session tracking
- [ ] Implement `get_container_mounts()` using `docker inspect`
- [ ] Implement `worktrees_match()` comparison logic
- [ ] Implement `count_active_ssh_sessions()` using state file
- [ ] Implement `is_safe_to_restart()` safety check
- [ ] Add user prompt for refresh confirmation
- [ ] Add session increment/decrement in SSH handler
- [ ] Wire up auto-detection logic in `handle_ssh()`
- [ ] Add progress indicators during refresh
- [ ] Handle edge cases (container not running, state corruption, etc.)
- [ ] Add unit tests for all new components
- [ ] Add integration tests for refresh scenarios

### Testing Scenarios
- [ ] Test auto-refresh with no active sessions
- [ ] Test blocked refresh with active sessions
- [ ] Test force-refresh disconnects other sessions
- [ ] Test --no-refresh skips detection
- [ ] Test state persistence across crashes
- [ ] Test concurrent SSH connection race conditions

## Non-Goals

- **Hot-swapping volumes**: Docker does not support adding volumes to running containers without restart
- **Cross-platform support**: This proposal focuses on Docker provider only (Tart/Vagrant may differ)
- **Bidirectional sync**: This does not implement file synchronization, only mount updates

## Alternatives Considered

### Alternative 1: SSHFS Network Filesystem
Mount new worktrees via SSHFS from inside container.

**Rejected because:**
- Requires SSH server on host
- Performance overhead
- Complex credential management
- Requires `sshfs` in container image

### Alternative 2: Docker CP Sync
Copy worktree files into container via `docker cp`.

**Rejected because:**
- Not a true mount (no live sync)
- Wastes disk space
- Requires bidirectional sync complexity
- Files get out of sync

### Alternative 3: Mount Parent Directory
Mount `~/.git/worktrees/` once to expose all future worktrees.

**Rejected because:**
- Exposes ALL worktrees (security/confusion)
- User's worktrees may not be in standard location
- Still requires symlink resolution logic

## Testing Strategy

### Unit Tests
- Mock `docker inspect` output parsing
- Test worktree comparison logic
- Test compose file volume updates

### Integration Tests
```rust
#[test]
fn test_ssh_refresh_mounts() {
    let temp_dir = TempDir::new()?;

    // 1. Create Git repo with worktree
    setup_git_repo_with_worktree(&temp_dir)?;

    // 2. Create VM
    run_vm_create(&temp_dir)?;

    // 3. Add NEW worktree
    add_worktree(&temp_dir, "feature-branch")?;

    // 4. SSH with refresh
    let result = run_vm_ssh_with_refresh(&temp_dir)?;

    // 5. Verify new worktree is mounted
    assert!(result.contains("/worktrees/feature-branch"));
}
```

## Documentation Updates

- [ ] Add `--refresh-mounts` to `vm ssh --help`
- [ ] Update `CLAUDE.md` with worktree workflow
- [ ] Add troubleshooting section for worktree issues
- [ ] Update README with worktree features

## Success Metrics

- ✅ Users can access new worktrees without `vm destroy`
- ✅ Refresh completes in <5 seconds
- ✅ Zero data loss during refresh
- ✅ Clear user feedback during refresh process

## Design Decisions

1. **Auto-refresh by default with safety checks**
   - Automatically detect new worktrees on every `vm ssh`
   - Prompt user before refreshing (unless `--no-refresh`)
   - Block refresh if other SSH sessions active (unless `--force-refresh`)
   - ~0.5s overhead for detection checks (acceptable trade-off)

2. **Session tracking via state file**
   - Store count in `~/.vm/state/{project-name}.json`
   - Increment on connect, decrement on disconnect
   - Hybrid check: state file + PTY count for reliability
   - Handle state corruption gracefully (fall back to PTY check)

3. **User prompts keep control transparent**
   - Always prompt before refresh (no surprise restarts)
   - Show which worktrees will be added
   - Clear instructions for `--force-refresh` and `--no-refresh`
   - Warning when force-disconnecting other sessions

4. **How to handle containers that are stopped?**
   - Skip stop step, just update compose and start

5. **What about non-Docker providers (Tart, Vagrant)?**
   - Start with Docker-only, extend to other providers if needed
   - Session tracking is provider-agnostic (state file approach)
   - Mount inspection will need provider-specific implementations

## References

- Git worktree docs: https://git-scm.com/docs/git-worktree
- Docker volume docs: https://docs.docker.com/storage/volumes/
- Existing worktree detection: `rust/vm-config/src/detector.rs`
- Compose generation: `rust/vm-provider/src/docker/compose.rs`
