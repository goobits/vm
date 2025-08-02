# VM Temp Enhancements TODO

## ⚠️ Major Issues (High Priority)

### Destructive Operations Without Confirmation
- [ ] Add process checking before destroy command
- [ ] Warn about data loss when removing volumes  
- [ ] Implement --force flag for bypassing confirmations
- [ ] Show what will be deleted before confirmation

## 🔧 Future Enhancements (Lower Priority)

### Mount Permissions System
- [ ] Implement :ro/:rw suffix parsing for mount permissions
- [ ] Add smart defaults for common read-only paths (.git, node_modules, vendor)
- [ ] Consider permission inheritance from parent directories
- [ ] Add `vm temp protect` / `vm temp unprotect` commands for changing permissions without remount

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