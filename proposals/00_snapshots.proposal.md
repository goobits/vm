## Problem

Developers cannot save and restore the state of their development environments. When switching between features, testing configurations, or recovering from mistakes, they must rebuild environments from scratch. Database backups exist (`vm db backup/restore`) but container filesystem changes, installed packages, and code modifications are lost on destroy.

## Solution(s)

Implement Docker Compose State Save approach that captures the complete environment state:

1. **Container Filesystem:** Use `docker commit` to save container changes as images
2. **Volume Data:** Create tar archives of all mounted volumes (complementing existing DB backups)
3. **Configuration:** Store vm.yaml and docker-compose.yml state
4. **Metadata:** Track snapshot timestamp, git commit hash, and user description

Storage location: `~/.config/vm/snapshots/<project>/<snapshot-name>/`

## Checklists

- [ ] **Core Implementation:**
    - [ ] Create `rust/vm/src/commands/snapshot.rs` module
    - [ ] Define `SnapshotManager` struct with snapshots_dir path
    - [ ] Define `SnapshotInfo` struct (name, created_at, container_image, volume_backups, config_files, git_commit, description)
    - [ ] Implement snapshot metadata serialization (JSON)
- [ ] **Snapshot Creation:**
    - [ ] Execute `docker commit <container>` to create image
    - [ ] Backup all volume data to tar archives
    - [ ] Copy vm.yaml and docker-compose.yml
    - [ ] Capture git commit hash if in git repository
    - [ ] Save metadata file
- [ ] **Snapshot Restoration:**
    - [ ] Load snapshot metadata
    - [ ] Recreate volumes from tar archives
    - [ ] Restore container from committed image
    - [ ] Apply saved configuration files
    - [ ] Start container with docker-compose
- [ ] **CLI Commands:**
    - [ ] Add `vm snapshot create <name>` command
    - [ ] Add `vm snapshot list` command
    - [ ] Add `vm snapshot restore <name>` command
    - [ ] Add `vm snapshot delete <name>` command
- [ ] **Provider Integration:**
    - [ ] Add `snapshot()` method to Provider trait
    - [ ] Add `restore()` method to Provider trait
    - [ ] Implement for DockerProvider
    - [ ] Return unsupported error for Vagrant/Tart providers
- [ ] **Documentation:**
    - [ ] Add snapshot section to docs/user-guide/configuration.md
    - [ ] Update README.md with snapshot feature
    - [ ] Add examples for common workflows
- [ ] **Verification:**
    - [ ] Test create/restore cycle preserves filesystem changes
    - [ ] Test multi-instance snapshot support
    - [ ] Verify volume data integrity
    - [ ] Ensure snapshots are portable (restore on clean system)
    - [ ] Run all existing tests to confirm no regressions

## Success Criteria

- Users can execute `vm snapshot create dev-working` and capture full environment state
- `vm snapshot restore dev-working` recreates the exact environment including installed packages, code changes, and volume data
- Snapshots support multi-instance VMs (`myproject-dev`, `myproject-staging`)
- Snapshot list shows name, creation time, size, and description
- All existing tests pass without modification

## Benefits

- Enables "save game" style development workflow
- Fast environment recovery from mistakes or failed experiments
- Share development environments across team members
- Test configuration changes with easy rollback
- Complements existing git worktree workflow for branch switching
