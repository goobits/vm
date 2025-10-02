# Proposal: User-Friendly Error Message Improvements

**Author:** Claude Code
**Date:** 2025-10-02
**Status:** Draft
**Target Version:** 2.0.6

## Executive Summary

While VM tool has unified error handling via `VmError` enum, many error messages lack actionable guidance for users. This proposal outlines improvements to make errors more user-friendly with clear next steps and helpful hints.

## Current State Analysis

### ✅ Strengths
- Unified `VmError` enum in `vm-core/src/error.rs`
- Consistent error categorization (Config, Provider, Dependency, etc.)
- Some errors already have helpful hints (via `vm doctor` command)
- vm-messages system provides centralized error templates

### ❌ Pain Points

**1. Dependency Errors Are Too Generic**
```rust
// Current
VmError::Dependency("Docker".into())
// Output: "Dependency not found: Docker"
```
**Problem:** No guidance on how to install or fix

**2. Config Errors Show Raw Parser Output**
```
Serialization error: mapping values are not allowed in this context at line 1 column 14
```
**Problem:** Technical jargon, no hint about what's wrong

**3. Provider Errors Lack Context**
```
Provider error: Container not found
```
**Problem:** User doesn't know which container or what to do

**4. Network Errors Are Ambiguous**
```
Network error: connection refused
```
**Problem:** Is it the service? Firewall? Network?

## Proposed Improvements

### 1. Enhanced Dependency Errors

**Goal:** Tell users exactly how to fix missing dependencies

#### Implementation

**Before:**
```
Error: Dependency not found: Docker
```

**After:**
```
❌ Docker is not installed or not in PATH

💡 Quick fixes:
   • Install Docker Desktop: https://docs.docker.com/get-docker/
   • Or use a different provider: vm config set provider vagrant
   • Verify installation: docker --version

🔍 Run 'vm doctor' for detailed diagnostics
```

#### Code Changes
- Add `DependencyError` struct with fields: `name`, `install_url`, `alternatives`, `verify_command`
- Update `VmError::Dependency` to include helpful context
- Add dependency-specific error builders in vm-messages

### 2. Improved Config Errors

**Goal:** Surface YAML syntax errors with helpful context

#### Implementation

**Before:**
```
Configuration error: Failed to load user config from: "test.yaml":
Serialization error: mapping values are not allowed in this context at line 1 column 14
```

**After:**
```
❌ Invalid YAML syntax in vm.yaml

📍 Error at line 1, column 14:
   1 | invalid: yaml: syntax
                    ^
   Unexpected ':' - YAML keys must be unique

💡 Common fixes:
   • Check for duplicate keys
   • Ensure proper indentation (use spaces, not tabs)
   • Quote strings containing special characters

📖 Valid example:
   project:
     name: my-project
   provider: docker

🔍 Validate online: https://www.yamllint.com/
```

#### Code Changes
- Create `ConfigParseError` with line/column highlighting
- Add YAML error translator to show context around error
- Include example snippets in error output

### 3. Provider-Specific Error Context

**Goal:** Help users understand and fix provider issues

#### Implementation

**Before:**
```
Provider error: Container not found
```

**After:**
```
❌ Container 'myproject-dev' not found

This usually means:
   • VM was destroyed or never created
   • Container name changed (try 'vm list' to see all VMs)
   • Docker daemon restarted (containers are stopped)

💡 Next steps:
   ✓ List all VMs: vm list
   ✓ Create new VM: vm create
   ✓ Check Docker status: vm doctor

🔍 Need to recover? Run 'vm create --force' to recreate
```

#### Code Changes
- Add `ProviderError` enum with specific variants
- Include suggested commands in error output
- Add recovery instructions for each error type

### 4. Network Error Diagnostics

**Goal:** Help users diagnose connectivity issues

#### Implementation

**Before:**
```
Network error: connection refused
```

**After:**
```
❌ Cannot connect to service on localhost:3080

Possible causes:
   • Service is not running (check with 'vm pkg status')
   • Port is blocked by firewall
   • Another service is using this port

💡 Troubleshooting:
   1. Check if service is running:
      vm pkg status

   2. Check if port is in use:
      lsof -i :3080  (macOS/Linux)
      netstat -ano | findstr :3080  (Windows)

   3. Start the service:
      vm pkg start

   4. Try a different port:
      vm config set package_registry.port 3081

🔍 Still stuck? Run 'vm doctor' for full diagnostics
```

#### Code Changes
- Add `NetworkError` struct with port, service name, diagnostics
- Include port-checking commands in error hints
- Auto-suggest alternative ports if available

### 5. Port Conflict Resolution

**Goal:** Automatically detect and resolve port conflicts

#### Implementation

**Before:**
```
Configuration validation failed:
  ❌ Service 'auth_proxy' is enabled but has no port specified
```

**After:**
```
⚠️  Port conflict detected

   Service 'postgresql' wants port 5432, but it's already in use

🔍 What's using this port:
   • Process: postgres.app (PID 12345)
   • You can stop it with: kill 12345

💡 Auto-fix options:
   1. Use a different port (recommended):
      vm config set services.postgresql.port 5433

   2. Stop the conflicting process:
      kill 12345

   3. Auto-assign available port:
      vm config ports --fix

✨ Quick fix: Run 'vm config ports --fix' to auto-resolve all conflicts
```

#### Code Changes
- Enhance port conflict detection to show process info
- Add `lsof`/`netstat` integration to identify processes
- Implement auto-fix with `--fix` flag

## Implementation Plan

### Phase 1: Core Error Infrastructure (2-3 hours)
- [ ] Create enhanced error types in `vm-core/src/error.rs`
- [ ] Add error context builders
- [ ] Update `VmError` to include helpful hints

### Phase 2: Dependency & Config Errors (2 hours)
- [ ] Implement enhanced dependency errors with install links
- [ ] Add YAML error parser with line highlighting
- [ ] Add example snippets to config errors

### Phase 3: Provider & Network Errors (2 hours)
- [ ] Add provider-specific error variants
- [ ] Implement network diagnostics
- [ ] Add recovery suggestions

### Phase 4: Port Conflict Resolution (1-2 hours)
- [ ] Enhance port conflict detection
- [ ] Add process identification
- [ ] Implement auto-fix functionality

### Phase 5: Testing & Documentation (1 hour)
- [ ] Test all error scenarios
- [ ] Update error handling tests
- [ ] Document new error patterns in CLAUDE.md

## Success Metrics

### User Experience
- ✅ 90% of errors include actionable next steps
- ✅ No raw parser output shown to users
- ✅ Every error links to relevant command or documentation

### Technical
- ✅ All new errors have unit tests
- ✅ Error context adds < 100 bytes overhead
- ✅ No performance regression in error paths

## Migration Strategy

### Backward Compatibility
- Existing `VmError` variants remain unchanged
- New error builders are additive, not breaking
- Old error messages enhanced, not replaced

### Rollout
1. Add new error infrastructure (non-breaking)
2. Migrate high-impact errors first (Docker, config)
3. Gradually enhance remaining errors
4. Update tests incrementally

## Examples of Enhanced Errors

### Docker Not Running
```
❌ Docker daemon is not running

💡 Start Docker:
   macOS:   Open Docker Desktop
   Linux:   sudo systemctl start docker
   Windows: Start Docker Desktop

✓ Verify: docker ps
🔍 More help: vm doctor
```

### Invalid Provider
```
❌ Unknown provider 'dockerr' in vm.yaml

Did you mean 'docker'?

💡 Valid providers:
   • docker   (lightweight containers, fast startup)
   • vagrant  (full VMs, maximum isolation)
   • tart     (macOS only, Apple Silicon optimized)

✓ Fix: vm config set provider docker
```

### Missing Config File
```
❌ Config file not found: vm.yaml

This is your first time! Let's create a configuration:

💡 Quick start:
   1. Generate config automatically:
      vm init

   2. Or create manually with example:
      vm init --example nodejs

   3. Or use defaults (no config needed):
      vm create

🎯 Most projects don't need a config file - try 'vm create' first!
```

## Risks & Mitigation

### Risk: Error Messages Too Verbose
**Mitigation:**
- Keep primary error concise (1 line)
- Hints are optional (can be disabled with `--quiet`)
- Progressive disclosure (brief → detailed → docs)

### Risk: Platform-Specific Commands
**Mitigation:**
- Detect OS and show relevant commands only
- Include fallback instructions
- Link to platform-specific docs

### Risk: Maintenance Burden
**Mitigation:**
- Centralize error templates in vm-messages
- Auto-generate error builders from templates
- Test error paths in CI

## Future Enhancements

### Auto-Fix Wizard (v2.1.0)
```
❌ Docker not running

💡 Auto-fix available!
   ? Start Docker automatically? (Y/n): y
   ✓ Starting Docker daemon...
   ✓ Docker is now running
   ✓ Retrying operation...
```

### Error Telemetry (v2.2.0)
- Track which errors users encounter most
- Prioritize improvements based on data
- A/B test error message clarity

### Interactive Error Handler (v2.3.0)
- Suggest commands as clickable links (terminal support)
- Offer to run fix commands automatically
- Learn from user choices to improve suggestions

## Conclusion

Improving error messages is a high-leverage enhancement that:
- ✅ Reduces user frustration
- ✅ Decreases support burden
- ✅ Improves onboarding experience
- ✅ Demonstrates care for UX

The proposed changes are incremental, backward-compatible, and measurable. Implementation can start immediately with Phase 1.

**Estimated Total Effort:** 8-10 hours
**User Impact:** High
**Technical Complexity:** Low-Medium
**Recommended Priority:** Medium-High

---

## Appendix: Error Categories Inventory

Current error distribution from codebase analysis:

| Error Type | Count | Has Hints | Needs Improvement |
|------------|-------|-----------|-------------------|
| Dependency | 3 | No | ✅ High priority |
| Config | ~15 | Partial | ✅ High priority |
| Provider | ~20 | No | ✅ Medium priority |
| Network | ~5 | No | ✅ Medium priority |
| Filesystem | ~10 | No | Low priority |
| Serialization | ~8 | No | ✅ High priority |
| Internal | ~5 | No | Low priority |

**Total Errors:** ~66 unique error sites
**High Priority:** ~26 errors (39%)
**Medium Priority:** ~25 errors (38%)
**Low Priority:** ~15 errors (23%)
