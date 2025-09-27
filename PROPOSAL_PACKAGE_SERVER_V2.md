# Package Server Integration Proposal v2

## Overview
Integrate the existing `vm-package-server` code into the main VM binary as `vm pkg` subcommands, eliminating redundant package downloads across VMs.

## Current State
- ✅ `vm-package-server` fully implemented in `/workspace/rust/vm-package-server/`
- ✅ Supports npm, pip, cargo registries with fallback to public registries
- ❌ Zero integration with main VM tool (exists as unused workspace member)

## Solution
Embed package server functionality directly into the main VM binary as `vm pkg` commands.

### Configuration
```yaml
# vm.yaml
package_registry: true  # Enable connection to host registry

# VMs auto-configure to use host.docker.internal:3080
```

### CLI Interface (Complete Command Set)
```bash
# Server Management
vm pkg start [--port] [--host] [--data] [--docker] [--no-config] [--foreground]
vm pkg stop
vm pkg status [--server]

# Package Operations
vm pkg add [--server] [--type]      # Publish from current directory
vm pkg remove [--server] [--force]  # Remove package
vm pkg list [--server]              # List packages

# Configuration
vm pkg config [show|get|set|reset|path]

# Shell Integration
vm pkg use [--shell] [--port] [--ttl]    # Output shell functions
vm pkg exec <command> [args...]          # One-off command via server

# NO standalone pkg-server binary - only vm pkg
```

### Implementation Tasks

1. **Move vm-package-server code into main VM binary**:
   - Create `rust/vm/src/commands/pkg/` module
   - Import all functionality from `vm-package-server/src/`
   - Remove `vm-package-server` as separate binary
   - Update `rust/vm/Cargo.toml` with package server dependencies

2. **CLI integration** (`rust/vm/src/commands/pkg.rs`):
   ```rust
   pub enum PkgCommand {
       Start { port: Option<u16>, docker: bool },
       Stop,
       Status,
       Add { server: String },
       List { server: Option<String> },
       Config(ConfigCommand),
   }
   ```

3. **VM provisioning integration** (`rust/vm-provider/src/*/provisioning.rs`):
   - Check if `package_registry: true` in config
   - Configure npm: `npm config set registry http://host.docker.internal:3080/npm/`
   - Configure pip: Add `http://host.docker.internal:3080/pypi/` to `~/.pip/pip.conf`
   - Configure cargo: Add registry to `~/.cargo/config.toml`

4. **Auto-start logic**:
   - `vm create` checks if registry running on port 3080
   - If not running and `package_registry: true`, prompt: "Start package registry? [Y/n]"
   - Auto-execute `vm pkg start` if user confirms

5. **Status integration**:
   ```bash
   vm status
   Platform Services:
     pkg-registry    ✓ Running (3080) - 47 packages cached

   my-app:
     Status:     Running
     Registry:   Connected (231ms, 98% cache hits)
     Services:   postgresql ✓, redis ✓
   ```

### Migration Strategy
- **Clean break**: Remove `vm-package-server` binary entirely
- **No legacy support**: All functionality moves to `vm pkg`
- **Workspace cleanup**: Remove `vm-package-server` from `rust/Cargo.toml` workspace members

### Expected Results
- **Unified CLI**: All VM functionality under single `vm` command
- **First VM**: Downloads packages normally (populates cache)
- **Subsequent VMs**: Install packages from local cache (0 bandwidth)
- **Bandwidth savings**: 90%+ for teams with multiple VMs
- **Speed improvement**: Package installs become nearly instant

### Technical Notes
- Package server code embedded in main VM binary (no separate process)
- Uses Docker's `host.docker.internal` for VM→host communication
- No changes to VM port ranges (registry uses fixed port 3080)
- Graceful fallback to public registries if host registry unavailable
- Registry persists between VM lifecycles (survives VM restarts)

### Success Criteria
- [ ] `pkg-server` binary removed from workspace
- [ ] All functionality accessible via `vm pkg` commands
- [ ] VM provisioning auto-configures package managers when `package_registry: true`
- [ ] Multi-VM bandwidth savings demonstrated (90%+ reduction)
- [ ] `vm status` shows registry connection health

### Breaking Changes
- `pkg-server` command line tool no longer exists
- All package server functionality available only via `vm pkg`
- Existing scripts using `pkg-server` must migrate to `vm pkg`