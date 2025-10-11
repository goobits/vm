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

Add a `--refresh-mounts` flag to `vm ssh` that intelligently detects new worktrees and performs a fast container restart if needed.

### User Experience

```bash
# After creating a new worktree
git worktree add ../feature-branch

# Option 1: Explicit refresh
vm ssh --refresh-mounts

⏳ New worktree detected, refreshing mounts...
✓ Stopped container (1s)
✓ Updated volumes (0.5s)
✓ Started container (1s)
🔗 Connected to VM

# Option 2: Auto-detect (future enhancement)
vm ssh

⏳ New worktree detected, refreshing mounts (2-3s)...
🔗 Connected to VM
```

## Technical Design

### Algorithm

```rust
pub fn ssh_with_mount_refresh(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
) -> Result<()> {
    // 1. Detect all current worktrees from Git
    let worktrees = detect_worktrees(&config)?;

    // 2. Get current container mounts
    let current_mounts = get_container_mounts(provider, container)?;

    // 3. Compare
    if worktrees_match(&worktrees, &current_mounts) {
        // No refresh needed - instant connection
        return handle_ssh(provider, container, None, None, config);
    }

    // 4. Refresh required
    println!("⏳ New worktree detected, refreshing mounts...");

    provider.stop(container)?;
    update_compose_volumes(container, &worktrees)?;
    provider.start(container)?;

    // 5. Connect
    handle_ssh(provider, container, None, None, config)
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

3. **Volume Update** (existing)
   - Modify `docker-compose.yml` to add new volumes
   - Use existing `vm-provider/src/docker/compose.rs` logic

4. **Fast Restart** (existing)
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

    /// Refresh volume mounts before connecting
    #[arg(long)]
    refresh_mounts: bool,  // ← NEW FLAG
},
```

### Command Handler Changes

**File**: `rust/vm/src/commands/vm_ops/interaction.rs`

- Add new function `handle_ssh_with_refresh()`
- Keep existing `handle_ssh()` for backward compatibility
- Route based on `refresh_mounts` flag

### Provider Interface (No Changes)

The existing provider interface already supports all needed operations:
- `provider.stop()` - Stop container
- `provider.start()` - Start container
- Docker-specific: `docker inspect` for mount inspection

## Implementation Phases

### Phase 1: Core Functionality (v1.0)
- [ ] Add `--refresh-mounts` flag to CLI
- [ ] Implement `get_container_mounts()` using `docker inspect`
- [ ] Implement `worktrees_match()` comparison logic
- [ ] Wire up restart logic in `handle_ssh()`
- [ ] Add unit tests

### Phase 2: Polish (v1.1)
- [ ] Add progress indicators during refresh
- [ ] Optimize comparison logic (cache previous state)
- [ ] Handle edge cases (container not running, etc.)
- [ ] Add integration tests

### Phase 3: Auto-Detection (v2.0 - Optional)
- [ ] Make `--refresh-mounts` the default behavior
- [ ] Add `--no-refresh` flag to skip detection
- [ ] Cache last-known worktree state to minimize checks

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

## Timeline

- **Week 1**: Phase 1 implementation
- **Week 2**: Testing and bug fixes
- **Week 3**: Documentation and polish

## Open Questions

1. Should `--refresh-mounts` be the default behavior?
   - **Recommendation**: No for v1, yes for v2 after validation

2. How to handle containers that are stopped?
   - **Recommendation**: Skip stop step, just update compose and start

3. Should we cache worktree state to avoid repeated checks?
   - **Recommendation**: Yes for v2, not needed for v1

4. What about non-Docker providers (Tart, Vagrant)?
   - **Recommendation**: Docker-only for v1, extend in future if needed

## References

- Git worktree docs: https://git-scm.com/docs/git-worktree
- Docker volume docs: https://docs.docker.com/storage/volumes/
- Existing worktree detection: `rust/vm-config/src/detector.rs`
- Compose generation: `rust/vm-provider/src/docker/compose.rs`
