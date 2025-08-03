# Smart Project Detection and Preset Management Proposal

## Executive Summary

This proposal outlines a system for automatically detecting project types and applying intelligent preset configurations when creating VMs. The goal is to reduce setup friction by automatically installing commonly needed tools based on the project context.

## Motivation

Currently, users must manually specify all tools and packages in `vm.yaml`. While this provides full control, it requires upfront knowledge of what tools they'll need. By detecting project types, we can:

1. **Reduce setup time** - Apply sensible defaults automatically
2. **Improve developer experience** - Less configuration needed to get started
3. **Maintain flexibility** - Users can still override any preset choices
4. **Support best practices** - Include commonly needed tools for each ecosystem

## Project Detection Strategy

### Detection Order (Fast to Slow)
```
1. Check for explicit vm.yaml (user knows what they want)
2. Quick file checks (< 100ms):
   - package.json → Node.js
   - requirements.txt/pyproject.toml → Python
   - Cargo.toml → Rust
   - go.mod → Go
   - Gemfile → Ruby
   - composer.json → PHP
3. Directory pattern checks:
   - .git/ → Version control tools
   - docker-compose.yml → Docker tools
   - k8s/,kubernetes/ → Kubernetes tools
4. Multi-project detection:
   - Multiple language markers → Polyglot preset
```

### Implementation Approach

#### 1. Project Detector Module
```bash
# shared/project-detector.sh
detect_project_type() {
    local project_dir="$1"
    local detected_types=()
    
    # Quick file-based detection
    [[ -f "$project_dir/package.json" ]] && detected_types+=("nodejs")
    [[ -f "$project_dir/requirements.txt" || -f "$project_dir/pyproject.toml" ]] && detected_types+=("python")
    [[ -f "$project_dir/Cargo.toml" ]] && detected_types+=("rust")
    [[ -f "$project_dir/go.mod" ]] && detected_types+=("go")
    
    # Return primary type or "multi" if multiple detected
    if [[ ${#detected_types[@]} -gt 1 ]]; then
        echo "multi:${detected_types[*]}"
    elif [[ ${#detected_types[@]} -eq 1 ]]; then
        echo "${detected_types[0]}"
    else
        echo "generic"
    fi
}
```

#### 2. Preset Definitions
```yaml
# configs/presets/nodejs.yaml
preset:
  name: "Node.js Development"
  description: "Optimized for JavaScript/TypeScript projects"
  
npm_packages:
  - prettier
  - eslint
  - typescript
  - ts-node
  - nodemon
  - npm-check-updates
  
pip_packages:
  - git-filter-repo  # Useful for any git project
  
services:
  redis:
    enabled: true  # Common for session storage
  postgresql:
    enabled: true  # Most common DB
    
environment:
  NODE_ENV: development
```

```yaml
# configs/presets/python.yaml
preset:
  name: "Python Development"
  description: "Optimized for Python projects"
  
pip_packages:
  - black
  - flake8
  - pytest
  - ipython
  - pip-tools
  - git-filter-repo
  - pre-commit
  
npm_packages:
  - pyright  # Python LSP
  
services:
  postgresql:
    enabled: true
  redis:
    enabled: true
    
environment:
  PYTHONDONTWRITEBYTECODE: "1"
```

```yaml
# configs/presets/multi.yaml
preset:
  name: "Multi-Language Development"
  description: "Supports multiple programming languages"
  
# Union of common tools
npm_packages:
  - prettier
  - eslint
  - npm-check-updates
  
pip_packages:
  - black
  - pytest
  - git-filter-repo
  - pre-commit
  
cargo_packages:
  - cargo-watch
  - cargo-edit
  
services:
  postgresql:
    enabled: true
  redis:
    enabled: true
  docker:
    enabled: true  # Likely needed for multi-project
```

#### 3. Generic/Base Preset
```yaml
# configs/presets/base.yaml
preset:
  name: "Generic Development"
  description: "General-purpose development environment"
  
pip_packages:
  - git-filter-repo
  - httpie
  - tldr
  
npm_packages:
  - prettier  # Works with many file types
  
aliases:
  ll: "ls -la"
  gs: "git status"
  
services:
  postgresql:
    enabled: false  # Let user opt-in
  redis:
    enabled: false
```

## User Experience Flow

### 1. New Project (No vm.yaml)
```bash
$ vm up
🔍 Detecting project type...
✅ Detected: Node.js project
📦 Applying Node.js preset (prettier, eslint, typescript...)
💡 You can customize this by creating a vm.yaml file

Creating VM with Node.js preset...
```

### 2. Explicit Opt-Out
```bash
$ vm up --no-preset
🚀 Creating VM with minimal configuration...
```

### 3. Show What Will Be Applied
```bash
$ vm up --dry-run
🔍 Detected: Python project
📋 Will apply Python preset:
  - pip: black, flake8, pytest, ipython, git-filter-repo
  - services: postgresql, redis
  - environment: PYTHONDONTWRITEBYTECODE=1

Continue? (Y/n):
```

### 4. Override Specific Parts
```yaml
# vm.yaml (partial config)
# Inherits from detected Python preset but overrides services
services:
  postgresql:
    enabled: false  # Override preset
  mongodb:
    enabled: true   # Use MongoDB instead
```

## Implementation Plan

### Phase 1: Core Detection (Week 1)
1. Create `shared/project-detector.sh`
2. Add detection logic to `vm.sh` up command
3. Create preset directory structure
4. Implement base and 2-3 language presets

### Phase 2: Preset System (Week 1-2)
1. Create preset loading mechanism
2. Implement preset merging with user config
3. Add --no-preset and --preset flags
4. Create preset documentation

### Phase 3: Enhanced Detection (Week 2)
1. Add framework detection (React, Django, Rails)
2. Add tool-specific presets (Docker, K8s)
3. Implement smart multi-project handling
4. Add preset recommendation system

### Phase 4: User Experience (Week 2-3)
1. Add interactive preset selection
2. Create `vm preset list` command
3. Add `vm preset show <name>` command
4. Implement preset customization guides

## Configuration Precedence

```
1. User's vm.yaml (highest priority)
2. Detected preset
3. Base preset
4. Schema defaults (lowest priority)
```

## Benefits

1. **Zero-config startup** - Just run `vm up` in any project
2. **Best practices by default** - Include commonly needed tools
3. **Discoverable** - Users learn about useful tools
4. **Customizable** - Full control when needed
5. **Maintainable** - Presets can be updated independently

## Example Preset Selection Logic

```bash
# In vm.sh up command
apply_smart_preset() {
    local project_dir="$1"
    local user_config="$2"
    
    # Skip if user has full config
    if [[ -f "$project_dir/vm.yaml" ]]; then
        return 0
    fi
    
    # Detect project type
    local project_type
    project_type=$(detect_project_type "$project_dir")
    
    echo "🔍 Detected project type: $project_type"
    
    # Load appropriate preset
    local preset_file="$SCRIPT_DIR/configs/presets/${project_type}.yaml"
    if [[ ! -f "$preset_file" ]]; then
        preset_file="$SCRIPT_DIR/configs/presets/base.yaml"
    fi
    
    # Merge preset with any partial user config
    merge_preset_with_config "$preset_file" "$user_config"
}
```

## Future Enhancements

1. **AI-Powered Detection** - Use file content analysis for better detection
2. **Community Presets** - Allow sharing custom presets
3. **Project Templates** - Full project scaffolding with VM config
4. **Learning System** - Track commonly installed packages per project type
5. **Version-Specific Presets** - Different tools for Python 2 vs 3, Node 16 vs 20

## Questions for Consideration

1. Should presets be minimal (only essentials) or comprehensive?
2. How to handle version conflicts between preset and project?
3. Should we detect and suggest based on Docker/CI configs?
4. How to make preset system extensible for organizations?