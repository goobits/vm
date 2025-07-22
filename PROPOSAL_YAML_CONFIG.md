# PROPOSAL: 3-Tier YAML Configuration System

## Overview
Replace current JSON-based vm.json with a flexible 3-tier YAML configuration system that supports local project mounting, user profiles, and secure environment management.

## Problem Statement
- Users can't easily share local npm packages (like BrowserTap) with VMs
- No way to set personal preferences across all VM projects
- JSON config is hard to read/edit and doesn't support comments
- Team configs get polluted with personal overrides

## Solution: 3-Tier Configuration

### 1. Project Config - `vm.yml` (tracked, team shared)
```yaml
name: my-app
provider: docker

services:
  - postgres
  - redis

ports:
  - 3000
  - 8080

environment:
  NODE_ENV: development
  API_KEY: ${API_KEY}  # Gets from environment
```

### 2. User Global - `~/.goobits-vm/config.yml` (personal defaults)
```yaml
defaults:
  timezone: America/Los_Angeles
  shell: zsh

local_projects:
  browsertap:
    path: ~/projects/browsertap
    auto_link: true

profiles:
  frontend:
    ports+: [6006] # Add storybook port
    services+: [node]
  backend:
    services-: [redis] # Don't need redis for backend work

global_mounts:
  - host: ~/.ssh
    container: ~/.ssh
    readonly: true
```

### 3. Project Local - `.goobits-vm.local.yml` (gitignored overrides)
```yaml
include_projects: [browsertap]

environment:
  API_KEY: your-secret-key

# Add a service just for this local session
services+:
  - mailhog

# Remove a port defined in the base config
ports-:
  - 8080
```

## Key Features

### Local Project Mounting
- Register local projects once in user config
- Selectively include per VM
- Auto npm link for CLI tools
- Solves BrowserTap sharing problem

### Profile System
- Preset configurations for different work types
- `vm create --profile=frontend` applies frontend settings
- Saves time on repeated setups

### Environment Enhancement
- `${VAR}` expansion from shell environment
- Secure secret handling (not committed to git)
- Per-project overrides in local config

### Smart Config Merging
Configuration is loaded and merged from four sources in a specific order, allowing for a clear hierarchy of overrides:

1. **Project Config** (`vm.yml`) - The base configuration for the team.
2. **User Global Config** (`~/.goobits-vm/config.yml`) - Your personal defaults for all projects.
3. **Selected Profile** (from User Global) - A specific context, e.g., `frontend`, applied via `--profile`.
4. **Project Local Config** (`.goobits-vm.local.yml`) - Final, git-ignored overrides and secrets.

**Merge Logic:**
- **Scalars (strings, numbers):** The last value loaded wins (e.g., a local value overrides a global one).
- **Objects:** Keys are deeply merged.
- **Arrays:** Replacement is the default behavior. To give fine-grained control, arrays can be modified using key suffixes:
    - `services: [mongo]` **replaces** any existing services list.
    - `services+: [mongo]` **adds** `mongo` to the list from the previous layers.
    - `services-: [postgres]` **removes** `postgres` from the list.
- **Variable Expansion:** `${VAR}` syntax is supported throughout and resolves from the shell environment.

### Complete Example
```yaml
# 1. vm.yml (base)
services: [postgres, redis]
ports: [3000, 8080]

# 2. ~/.goobits-vm/config.yml (user)
defaults:
  timezone: America/Los_Angeles

# 3. Profile: frontend (from user config)
services+: [node]
ports+: [6006]

# 4. .goobits-vm.local.yml (overrides)
services+: [mailhog]
services-: [redis]
ports-: [8080]

# Final result:
services: [postgres, node, mailhog]  # redis removed, node + mailhog added
ports: [3000, 6006]                  # 8080 removed, 6006 added
```

## CLI Enhancements
```bash
vm create --profile=backend     # Apply profile
vm config --show               # Display merged config
vm config --check              # Validate all configs
vm migrate-config              # Convert vm.json → vm.yml
vm config --init-user          # Create ~/.goobits-vm/config.yml
```

## Implementation Plan

### Phase 1: Core YAML System
- Add YAML parser (`yq`). The `install.sh` script must check if `yq` is installed and guide the user if it is missing.
- Implement 4-tier config merger with support for `+` and `-` array modifiers.
- Add variable expansion.
- Create migration tool (`vm migrate-config`).

### Phase 2: Local Project Support
- Implement local_projects registry
- Auto-generate volume mounts
- Add npm link automation

### Phase 3: Profiles & CLI
- Build profile system
- Add new CLI commands
- Enhance Docker Compose generation

## Benefits
- ✅ Solves local package sharing (BrowserTap use case)
- ✅ Clean separation: team vs personal vs secret configs
- ✅ YAML = human readable, supports comments
- ✅ Flexible profiles for different project types
- ✅ Secure environment variable handling
- ✅ Backwards compatible (migration tool)

## File Changes
```
+ vm.yml                      # New project config format
+ ~/.goobits-vm/config.yml    # User global config
+ .goobits-vm.local.yml       # Project local overrides
+ lib/yaml-parser.sh          # YAML parsing utilities
+ lib/config-merger.sh        # 4-tier merge logic
+ lib/migrate-config.sh       # JSON→YAML migration
~ vm.sh                       # Add new commands
~ generate-config.sh          # YAML generation
```

## Migration Strategy
1. Keep vm.json support during transition
2. Add `vm migrate-config` command
3. Deprecation notice for JSON format
4. Eventually remove JSON support

This proposal maintains the project's flexibility while solving the core local development workflow issues.