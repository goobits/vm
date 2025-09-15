# Comprehensive Provider Refactor Proposal

**STATUS: SUPERSEDED** - This proposal has been superseded by the migration to Rust. The shell-based provider system described here has been replaced with a Rust implementation in the `rust/vm-provider/` crate.

---

## Current State Analysis

### Existing Structure
- `shared/provider-interface.sh` (654 lines) - Already has routing but mixed with implementation
- `providers/docker/` - Scripts but no unified interface
- `providers/tart/tart-provider.sh` (721 lines) - Monolithic implementation
- `providers/vagrant/` - Config files only, no command implementation
- `vm.sh` - Contains 1,368 lines of Docker-specific code

### Problems
1. **Mixed concerns** - provider-interface.sh contains Docker/Vagrant implementations
2. **Inconsistent interfaces** - Each provider has different structure
3. **Duplication** - Docker code in both vm.sh and provider-interface.sh
4. **No clear boundaries** - Provider logic scattered across files

## Proposed New Architecture

```
providers/
├── base.sh                    # Abstract provider interface (50 lines)
├── registry.sh                 # Provider registration/loading (100 lines)
├── docker/
│   ├── provider.sh            # Implements base interface (150 lines)
│   ├── commands/              # Modular command implementations
│   │   ├── lifecycle.sh       # start, stop, destroy (300 lines)
│   │   ├── access.sh          # ssh, exec, shell (200 lines)
│   │   ├── info.sh            # status, logs, list (150 lines)
│   │   └── provision.sh       # provision, reload (400 lines)
│   ├── utils.sh               # Docker-specific utilities (200 lines)
│   └── config.sh              # Docker compose generation (moved from docker-provisioning-simple.sh)
├── tart/
│   ├── provider.sh            # Implements base interface (150 lines)
│   ├── commands/              # Split from tart-provider.sh
│   │   ├── lifecycle.sh       # VM lifecycle (200 lines)
│   │   ├── access.sh          # SSH access (100 lines)
│   │   └── info.sh            # Status/info (100 lines)
│   └── utils.sh               # Tart utilities (150 lines)
└── vagrant/
    ├── provider.sh            # Implements base interface (150 lines)
    ├── commands/              # Extract from provider-interface.sh
    │   ├── lifecycle.sh       # Vagrant lifecycle (200 lines)
    │   ├── access.sh          # SSH/exec (150 lines)
    │   └── provision.sh       # Provisioning (100 lines)
    └── config.rb              # Keep existing Vagrant config
```

## Files to Rewrite/Refactor

1. **Split `shared/provider-interface.sh`:**
   - Extract Docker implementation → `providers/docker/commands/`
   - Extract Vagrant implementation → `providers/vagrant/commands/`
   - Keep only routing logic → becomes `providers/registry.sh`

2. **Extract from `vm.sh`:**
   - All `docker_*` functions → `providers/docker/commands/`
   - Remove 1,368 lines of Docker code

3. **Split `providers/tart/tart-provider.sh`:**
   - Core logic → `providers/tart/provider.sh`
   - Commands → `providers/tart/commands/`

4. **Consolidate `providers/docker/docker-provisioning-simple.sh`:**
   - Move to `providers/docker/config.sh`
   - Integrate with provider interface

## New Provider Interface (`providers/base.sh`)

```bash
#!/bin/bash
# Abstract provider interface - all providers must implement these

provider_name() { echo "base"; }
provider_available() { return 1; }
provider_validate_config() { return 0; }

# Lifecycle commands
provider_create() { error "Not implemented"; }
provider_start() { error "Not implemented"; }
provider_stop() { error "Not implemented"; }
provider_destroy() { error "Not implemented"; }
provider_restart() { provider_stop "$@" && provider_start "$@"; }

# Access commands
provider_ssh() { error "Not implemented"; }
provider_exec() { error "Not implemented"; }
provider_shell() { provider_ssh "$@"; }

# Info commands
provider_status() { error "Not implemented"; }
provider_logs() { error "Not implemented"; }
provider_list() { error "Not implemented"; }

# Management commands
provider_provision() { error "Not implemented"; }
provider_reload() { provider_restart "$@" && provider_provision "$@"; }
```

## Migration Strategy

### Phase 1: Create New Structure (Non-breaking)
1. Create `providers/base.sh` interface
2. Create `providers/registry.sh` for loading
3. Create provider directories with empty files
4. Copy existing functions to new locations (keep originals)

### Phase 2: Implement Providers
1. Implement Docker provider using existing functions
2. Extract Vagrant functions from provider-interface.sh
3. Split Tart provider into modules
4. Test each provider independently

### Phase 3: Wire Up New System
1. Update vm.sh to use provider registry
2. Remove old Docker functions from vm.sh
3. Replace provider-interface.sh with registry.sh
4. Remove duplicated code

### Phase 4: Cleanup
1. Delete old implementations
2. Update tests to use new structure
3. Update documentation

## Code Reduction Estimates

- **vm.sh:** 3,288 → ~1,600 lines (-51%)
- **provider-interface.sh:** 654 → 100 lines (becomes registry.sh)
- **tart-provider.sh:** 721 → ~500 lines (split into modules)
- **Total reduction:** ~2,400 lines eliminated through better organization

## Benefits

1. **Clear boundaries** - Each provider is self-contained
2. **Consistent interface** - All providers implement base.sh
3. **Modular commands** - Easy to test/modify individual commands
4. **No duplication** - Single source of truth for each provider
5. **Extensibility** - Easy to add new providers
6. **Maintainability** - Provider experts work in isolation