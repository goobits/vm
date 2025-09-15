# VM Temp Enhancements TODO

**STATUS: ARCHIVED** - These enhancements have been implemented in the Rust migration. The shell-based vm-temp functionality described here has been replaced with `rust/vm-temp/`.

## ✅ All Major Issues Resolved!

### ✅ COMPLETED: Destructive Operations Without Confirmation  
- [x] ~~Add process checking before destroy command~~ - Implemented with check_running_processes()
- [x] ~~Warn about data loss when removing volumes~~ - Added clear warnings with reassuring host file safety message  
- [x] ~~Implement --force flag for bypassing confirmations~~ - Added --force/-f flag support
- [x] ~~Show what will be deleted before confirmation~~ - Interactive prompt shows container, volumes, and process info

## 🔧 Future Enhancements (Lower Priority)

### ✅ COMPLETED: Mount Permissions System
- [x] ~~Implement :ro/:rw suffix parsing for mount permissions~~ - Fully implemented in mount-utils.sh and temporary-vm-utils.sh
- [ ] Add smart defaults for common read-only paths (.git, node_modules, vendor)
- [ ] Consider permission inheritance from parent directories
- [ ] Add `vm temp protect` / `vm temp unprotect` commands for changing permissions without remount

### Multiple Temp VMs
- [ ] Support multiple named temp VMs (vm temp create --name review)
- [ ] Implement `vm temp list` to show all temp VMs - **Note: Command shown in help but NOT implemented**
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