# VM Temp Enhancements TODO

## 🚨 Critical Issues (Immediate Priority)

### 1. Container Stop Without Recovery
- [ ] Fix docker-compose error handling in update_temp_vm_with_mounts (Lines 302-347)
- [ ] Add proper error messages and recovery guidance when docker-compose fails
- [ ] Remove silent failure pattern with `|| docker-compose up -d`

### 2. State File Race Conditions
- [ ] Implement file locking for all state file operations
- [ ] Add atomic read/write operations to prevent corruption
- [ ] Handle concurrent vm temp commands safely

### 3. Orphaned Docker Resources
- [ ] Clean up volumes/networks on container creation failure (Lines 1462-1472)
- [ ] Add resource cleanup in error paths
- [ ] Implement periodic orphan resource detection and cleanup

## ⚠️ Major Issues (High Priority)

### 4. No Mount Validation
- [ ] Verify mounts work after starting stopped container
- [ ] Check if mount sources exist when reading state file
- [ ] Add warnings for "phantom" mounts that no longer exist
- [ ] Validate mount accessibility inside container

### 5. Destructive Operations Without Confirmation
- [ ] Add process checking before destroy command
- [ ] Warn about data loss when removing volumes
- [ ] Implement --force flag for bypassing confirmations
- [ ] Show what will be deleted before confirmation

### 6. Missing Error Handling
- [ ] Add comprehensive error messages for Docker operations
- [ ] Provide recovery instructions on failures
- [ ] Log errors to help with debugging
- [ ] Implement retry logic for transient failures

## ✅ Completed Features
- [x] JSON to YAML migration (implemented in vm migrate command)
- [x] Fix MOUNTS display in vm list to show paths instead of objects
- [x] Fix vm temp mount to use YAML config format
- [x] Change vm temp mount to restart container instead of recreating

## 🔧 Future Enhancements (Lower Priority)

### Mount Permissions System
- [ ] Implement :ro/:rw suffix parsing for mount permissions
- [ ] Add smart defaults for common read-only paths (.git, node_modules, vendor)
- [ ] Consider permission inheritance from parent directories
- [ ] Add `vm temp protect` / `vm temp unprotect` commands for changing permissions without remount

## Multiple Temp VMs
- [ ] Support multiple named temp VMs (vm temp create --name review)
- [ ] Implement `vm temp list` to show all temp VMs
- [ ] Add temp VM naming conventions and validation

## Performance Optimizations
- [ ] Implement mount caching to speed up container recreation
- [ ] Add --quick flag to skip confirmation prompts
- [ ] Optimize container restart time with pre-built layers

### Multiple Temp VMs
- [ ] Support multiple named temp VMs (vm temp create --name review)
- [ ] Implement `vm temp list` to show all temp VMs
- [ ] Add temp VM naming conventions and validation

### Performance Optimizations
- [ ] Implement mount caching to speed up container recreation
- [ ] Add --quick flag to skip confirmation prompts
- [ ] Optimize container restart time with pre-built layers

### Developer Experience
- [ ] Add mount templates (vm temp --template fullstack)
- [ ] Implement mount groups for common project types
- [ ] Add shell completion for mount paths
- [ ] Create mount history/suggestions based on usage

### Integration Features
- [ ] Support for .vmtempignore file to exclude paths
- [ ] Integration with .gitignore for smart mount suggestions
- [ ] Auto-detect project type and suggest appropriate mounts

### Advanced Features
- [ ] Network mount support (SSHFS, NFS)
- [ ] Shared temp VMs between team members
- [ ] Temp VM snapshots for quick state restoration
- [ ] Mount synchronization with file watchers

### Security Enhancements
- [ ] Audit log for mount operations
- [ ] Mount path validation and sandboxing
- [ ] Read-only by default mode for security-conscious users