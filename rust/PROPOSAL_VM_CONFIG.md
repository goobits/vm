# vm-config: YAML Configuration Processor

## Problem
- `validate-config.sh` + `config-processor.sh` (1000+ lines) are complex
- Multiple yq calls with cross-platform compatibility issues
- Slow config merging and validation

## Current Implementation Analysis
**Has 6 commands:**
1. `merge` - Merge multiple configs
2. `validate` - Validate config file
3. `preset` - Detect/apply presets
4. `convert` - Format conversion
5. `process` - Full VM Tool logic (defaults + presets + user)
6. `query` - Extract field values

## Recommendation: KEEP ALL COMMANDS

**Analysis shows ALL 6 commands are actively used:**
- `merge` - Used in `/workspace/shared/config-processor.sh`
- `preset` - Used in `/workspace/shared/config-processor.sh`
- `convert` - Used in `/workspace/bin/vm-config-wrapper.sh` for yq compatibility
- `validate`, `query`, `process` - Core functionality

**Cannot simplify without breaking existing shell scripts and yq wrapper.**

## Benefits of Current Implementation
- Already works as yq replacement via wrapper
- Supports all VM tool configuration needs
- Well-tested with existing shell scripts

## CLI Interface Status
**Current 6 commands are correctly scoped - KEEP ALL:**
- `validate` - Config validation
- `query` - Field extraction (replaces `yq -r`)
- `process` - Full merge logic
- `merge` - Direct config merging
- `preset` - Preset detection/application
- `convert` - Format conversion for yq compatibility

**Output format:** Already matches shell requirements