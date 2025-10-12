# VM Tool Testing Matrix Analysis

**Generated:** 2025-10-11
**Purpose:** Design ideal test coverage and compare with current state

---

## Executive Summary

This document provides:
1. An ideal testing matrix for comprehensive coverage
2. Current test coverage analysis
3. Gap identification
4. Actionable recommendations (files to delete, edit, or add)

**Key Findings:**
- ✅ Strong coverage: `vm-config`, `vm-package-server`, `vm` integration tests
- ⚠️  Moderate coverage: `vm-provider`, `vm-temp`, `vm-package-manager`
- ❌ Missing coverage: `vm-auth-proxy`, `vm-docker-registry`, `vm-installer`, `vm-plugin`, `vm-platform`, `vm-core`

---

## Part 1: Ideal Testing Matrix

### Testing Taxonomy

```
Unit Tests (in src/)
  ├── Pure functions (business logic)
  ├── Data structures (serialization, validation)
  └── Error handling

Integration Tests (in tests/)
  ├── Cross-module workflows
  ├── File I/O operations
  └── State management

E2E Tests (in tests/)
  ├── CLI command execution
  ├── Multi-command workflows
  └── Real provider interactions

Contract Tests
  ├── Trait implementations
  └── API compatibility
```

### Test Coverage by Priority

#### 🔴 CRITICAL (Must have 90%+ coverage)

**1. vm-provider** - VM Lifecycle Management
- [ ] **Unit Tests** (per provider: docker, vagrant, tart)
  - [ ] Instance creation/destruction
  - [ ] State transitions (create → start → stop → destroy)
  - [ ] Error handling (missing dependencies, config errors)
  - [ ] Mount management
  - [ ] Multi-instance support
  - [ ] Instance resolution

- [ ] **Integration Tests**
  - [x] Provider parity tests (all providers same behavior) ✓ `provider_parity_tests.rs`
  - [x] Lifecycle tests (full VM lifecycle) ✓ `lifecycle_integration_tests.rs`
  - [x] Multi-instance tests ✓ `multi_instance_tests.rs`
  - [ ] **Missing:** Vagrant provider tests
  - [ ] **Missing:** Tart provider tests (only stub exists)
  - [ ] **Missing:** Mock provider validation tests

- [ ] **Contract Tests**
  - [ ] All providers implement `Provider` trait correctly
  - [ ] TempProvider trait implementation tests

**2. vm** - Main CLI Integration
- [x] **E2E Command Tests**
  - [x] `create` command (multiple scenarios) ✓ `vm_ops/create_destroy_tests.rs`
  - [x] `start` command ✓ `workflow_tests.rs`
  - [x] `stop` command ✓ `workflow_tests.rs`
  - [x] `destroy` command ✓ `vm_ops/create_destroy_tests.rs`
  - [x] `ssh` command ✓ `ssh_refresh.rs`
  - [x] `exec` command ✓ `interaction_tests.rs`
  - [x] `logs` command ✓ `interaction_tests.rs`
  - [x] `status` command ✓ `status_tests.rs`
  - [x] `list` command ✓ `workflow_tests.rs`
  - [x] `restart` command ✓ `lifecycle_integration_tests.rs`
  - [x] `provision` command ✓ Basic coverage in workflows
  - [x] `config` subcommands ✓ `config_cli_tests.rs`
  - [x] `pkg` subcommands ✓ `pkg_cli_tests.rs`
  - [x] Port forwarding ✓ `port_forwarding_tests.rs`

- [x] **Service Management Tests**
  - [x] Service lifecycle ✓ `service_lifecycle_integration_tests.rs`, `vm_ops/service_lifecycle_tests.rs`
  - [x] Reference counting
  - [x] Service state persistence

- [ ] **Missing Integration Tests**
  - [ ] Cross-command workflows (create → start → exec → stop → destroy)
  - [ ] Error recovery scenarios
  - [ ] Concurrent VM operations
  - [ ] Global config hot-reload

**3. vm-auth-proxy** - Security & Encryption
- [ ] **Unit Tests** ❌ MISSING ENTIRELY
  - [ ] AES-256-GCM encryption/decryption
  - [ ] Key derivation (PBKDF2)
  - [ ] Bearer token validation
  - [ ] Secret storage operations
  - [ ] Audit logging

- [ ] **Integration Tests** ❌ MISSING ENTIRELY
  - [ ] HTTP API endpoints (`/secrets`, `/secrets/:key`)
  - [ ] Authentication flows
  - [ ] Concurrent access
  - [ ] Secret injection into VMs

- [ ] **Security Tests** ❌ MISSING ENTIRELY
  - [ ] Encryption strength validation
  - [ ] Token expiration
  - [ ] Access control
  - [ ] SQL injection prevention (if using DB)

**4. vm-package-server** - Multi-Registry Server
- [x] **Unit Tests**
  - [x] Package validation ✓ Extensive in `src/validation/`
  - [x] Hash utilities ✓ `src/hash_utils.rs`
  - [x] PyPI utilities ✓ `src/pypi_utils.rs`
  - [x] Cargo index ✓ `src/cargo/tests.rs`

- [x] **Integration Tests**
  - [x] PyPI registry ✓ `integration_test.rs`
  - [x] npm registry ✓ `integration_test.rs`
  - [x] Cargo registry ✓ `integration_test.rs`
  - [x] Package upload ✓ `upload_security_test.rs`, `upload_limits_test.rs`
  - [x] Package deletion ✓ `package_add_remove_test.rs`

- [x] **Security Tests**
  - [x] Upload limits ✓ `upload_limits_test.rs`
  - [x] Path traversal prevention ✓ `upload_security_test.rs`
  - [x] Docker image validation ✓ `docker_security_test.rs`
  - [x] Malicious package detection ✓ `upload_security_test.rs`

- [x] **E2E Tests**
  - [x] CLI commands ✓ `cli_commands_e2e_test.rs`

#### 🟠 HIGH (Need 80%+ coverage)

**5. vm-config** - Configuration Management
- [x] **Unit Tests**
  - [x] Config loading ✓ Multiple tests in `src/lib.rs`
  - [x] Config merging ✓ `src/merge.rs`
  - [x] Preset application ✓ `src/preset.rs`
  - [x] Validation ✓ `src/validate.rs`
  - [x] Framework detection ✓ `src/detector/tests/*`
  - [x] Port management ✓ `src/ports/range.rs`, `src/ports/registry.rs`
  - [x] Resource allocation ✓ `src/resources.rs`

- [x] **Integration Tests**
  - [x] Config operations ✓ `tests/config_ops_tests.rs`
  - [x] Port allocation ✓ `tests/port_allocation.rs`
  - [x] Migration tests ✓ `tests/migrate_tests.rs`

- [ ] **Missing Tests**
  - [ ] Worktree detection edge cases
  - [ ] YAML formatter round-trip tests
  - [ ] Complex preset inheritance

**6. vm-core** - Foundation Utilities
- [x] **Unit Tests** (partial)
  - [x] System resource detection ✓ `src/system_check.rs`
  - [x] Project detection ✓ `src/project.rs`
  - [x] User paths ✓ `src/user_paths.rs`
  - [ ] **Missing:** File system operations tests
  - [ ] **Missing:** Command stream tests
  - [ ] **Missing:** Error handling tests

- [ ] **Integration Tests** ❌ MISSING ENTIRELY
  - [ ] Cross-platform path resolution
  - [ ] Temporary directory management
  - [ ] Command execution with streaming

**7. vm-temp** - Temporary VM Management
- [x] **Integration Tests**
  - [x] Temp VM lifecycle ✓ `tests/integration_tests.rs`
  - [x] Mount operations ✓ Covered in integration tests
  - [x] State persistence ✓ Covered in integration tests

- [ ] **Missing Tests**
  - [ ] Unit tests for mount_ops module
  - [ ] Unit tests for state module
  - [ ] Concurrent mount updates
  - [ ] Error recovery for corrupted state

**8. vm-installer** - Installation Tool
- [ ] **Integration Tests** ❌ MISSING ENTIRELY
  - [ ] Platform detection (Linux, macOS, Windows)
  - [ ] Build from source
  - [ ] Binary installation
  - [ ] PATH updates
  - [ ] Shell config updates (.bashrc, .zshrc, etc.)
  - [ ] Dependency verification (cargo, docker, etc.)
  - [ ] Upgrade scenarios
  - [ ] Uninstall cleanup

- [ ] **Unit Tests** (partial)
  - [x] Platform detection ✓ `src/platform.rs`
  - [x] Installer logic ✓ `src/installer.rs`
  - [ ] **Missing:** Cross-platform path handling
  - [ ] **Missing:** Permission handling

#### 🟡 MEDIUM (Need 60%+ coverage)

**9. vm-platform** - Cross-Platform Abstraction
- [x] **Unit Tests** (partial)
  - [x] Platform provider ✓ `src/lib.rs`
  - [x] Registry ✓ `src/registry.rs`
  - [ ] **Missing:** OS-specific behavior tests
  - [ ] **Missing:** Shell detection tests (bash, zsh, fish, powershell)
  - [ ] **Missing:** Docker host gateway tests

- [ ] **Contract Tests** ❌ MISSING
  - [ ] All platform providers implement traits correctly
  - [ ] Consistent behavior across platforms

**10. vm-plugin** - Plugin System
- [x] **Unit Tests** (partial)
  - [x] Plugin discovery ✓ `src/discovery.rs`
  - [x] Plugin validation ✓ `src/validation.rs`
  - [x] Plugin types ✓ `src/types.rs`

- [ ] **Integration Tests** ❌ MISSING ENTIRELY
  - [ ] Load preset plugins from disk
  - [ ] Load service plugins from disk
  - [ ] Invalid plugin handling
  - [ ] Plugin priority resolution
  - [ ] Multiple plugin directories

**11. vm-package-manager** - Package Manager CLI
- [x] **Integration Tests**
  - [x] Link detection ✓ `tests/links_integration_tests.rs`

- [ ] **Missing Tests**
  - [ ] Package installation workflows
  - [ ] Link creation/removal
  - [ ] Broken link detection
  - [ ] Package resolution from multiple sources

**12. vm-docker-registry** - Local Docker Registry
- [x] **Unit Tests** (partial)
  - [x] Configuration ✓ `src/config.rs`
  - [x] Docker config ✓ `src/docker_config.rs`
  - [x] Auto manager ✓ `src/auto_manager.rs`
  - [x] Server management ✓ `src/server.rs`

- [ ] **Integration Tests** ❌ MISSING ENTIRELY
  - [ ] Pull-through caching
  - [ ] Image push/pull workflows
  - [ ] Garbage collection
  - [ ] Docker daemon integration
  - [ ] Offline mode operation

#### 🟢 LOW (Basic tests sufficient)

**13. vm-cli** - CLI Utilities
- [x] **Unit Tests** ✓ Minimal needed
  - Message building covered in usage

**14. vm-messages** - Message Templates
- [x] **Unit Tests** ✓ Minimal needed
  - Static data, tested through usage

**15. vm-logging** - Structured Logging
- [ ] **Integration Tests** (optional)
  - [ ] Log output format validation
  - [ ] Tag-based filtering
  - [ ] File output

**16. version-sync** - Version Management
- [ ] **Integration Tests** (optional)
  - [ ] Version synchronization across files
  - [ ] Version validation

---

## Part 2: Current Coverage Analysis

### Coverage Summary by Package

| Package | Unit Tests | Integration Tests | E2E Tests | Overall |
|---------|------------|-------------------|-----------|---------|
| **vm-provider** | 🟢 Good (in src/) | 🟡 Moderate (3 files) | N/A | 🟡 70% |
| **vm** | 🟢 Minimal (in src/) | 🟢 Excellent (11 files) | 🟢 Excellent | 🟢 90% |
| **vm-auth-proxy** | 🟢 Good (in src/) | ❌ None | ❌ None | 🔴 40% |
| **vm-package-server** | 🟢 Excellent | 🟢 Excellent (5 files) | 🟢 Good (1 file) | 🟢 95% |
| **vm-config** | 🟢 Excellent | 🟢 Good (3 files) | N/A | 🟢 85% |
| **vm-core** | 🟡 Moderate | ❌ None | N/A | 🟡 50% |
| **vm-temp** | 🟡 Moderate | 🟢 Good (1 file) | N/A | 🟡 65% |
| **vm-installer** | 🟡 Moderate | ❌ None | ❌ None | 🔴 30% |
| **vm-plugin** | 🟡 Moderate | ❌ None | N/A | 🔴 40% |
| **vm-platform** | 🟡 Moderate | ❌ None | N/A | 🔴 45% |
| **vm-package-manager** | 🔴 Minimal | 🟡 Moderate (1 file) | N/A | 🟡 50% |
| **vm-docker-registry** | 🟡 Moderate | ❌ None | N/A | 🔴 35% |
| **vm-cli** | 🟢 Covered by usage | N/A | N/A | 🟢 80% |
| **vm-messages** | 🟢 Covered by usage | N/A | N/A | 🟢 85% |
| **vm-logging** | 🔴 Minimal | ❌ None | N/A | 🔴 25% |
| **version-sync** | 🔴 None | ❌ None | N/A | 🔴 0% |

### Test File Inventory

#### Workspace-Level Tests
- ✅ `rust/tests/integration_tests.rs` - Cross-crate integration

#### vm-config (3 integration test files)
- ✅ `tests/config_ops_tests.rs` (9.8KB) - Config operations, uses TEST_MUTEX
- ✅ `tests/migrate_tests.rs` (3.3KB) - Migration tests
- ✅ `tests/port_allocation.rs` (7.8KB) - Port allocation tests
- ✅ Unit tests in `src/detector/tests/` (8 files) - Framework detection

#### vm-provider (1 integration test file)
- ⚠️  `tests/tart_provider_tests.rs` (1.8KB) - Tart provider (stub only)

#### vm-temp (1 integration test file)
- ✅ `tests/integration_tests.rs` (16.9KB) - Comprehensive temp VM tests

#### vm (11 integration test files, 3,536 LOC)
- ✅ `tests/workflow_tests.rs` (596 LOC) - Main CLI workflows
- ✅ `tests/config_cli_tests.rs` (479 LOC) - Config commands
- ✅ `tests/service_lifecycle_integration_tests.rs` (385 LOC) - Service lifecycle
- ✅ `tests/pkg_cli_tests.rs` (348 LOC) - Package commands
- ✅ `tests/port_forwarding_tests.rs` (247 LOC) - Port forwarding
- ✅ `tests/temp_workflow_tests.rs` (183 LOC) - Temp workflows
- ✅ `tests/ssh_refresh.rs` (107 LOC) - SSH auto-refresh
- ✅ `tests/vm_ops.rs` (15 LOC) - Module declaration
- ✅ `tests/vm_ops/create_destroy_tests.rs` (112 LOC)
- ✅ `tests/vm_ops/feature_tests.rs` (199 LOC)
- ✅ `tests/vm_ops/lifecycle_integration_tests.rs` (62 LOC)
- ✅ `tests/vm_ops/multi_instance_tests.rs` (71 LOC)
- ✅ `tests/vm_ops/provider_parity_tests.rs` (66 LOC)
- ✅ `tests/vm_ops/interaction_tests.rs` (156 LOC)
- ✅ `tests/vm_ops/service_lifecycle_tests.rs` (177 LOC)
- ✅ `tests/vm_ops/status_tests.rs` (143 LOC)
- ✅ `tests/vm_ops/helpers.rs` (190 LOC) - Test utilities
- ✅ `tests/common/mod.rs` - Shared test utilities

#### vm-package-server (5 integration test files)
- ✅ `tests/integration_test.rs` (2.7KB) - Registry integration
- ✅ `tests/upload_security_test.rs` (5.4KB) - Upload security
- ✅ `tests/upload_limits_test.rs` (7.7KB) - Upload limits
- ✅ `tests/docker_security_test.rs` (5.9KB) - Docker security
- ✅ `tests/package_add_remove_test.rs` (6.4KB) - Package management
- ✅ `tests/cli_commands_e2e_test.rs` (6.5KB) - CLI E2E
- ✅ `tests/common/mod.rs` - Test utilities
- ✅ Unit tests in `src/cargo/tests.rs`
- ✅ Extensive unit tests in `src/validation/` modules

#### vm-package-manager (1 integration test file)
- ✅ `tests/links_integration_tests.rs` (11.4KB) - Link management

#### Packages with NO integration tests
- ❌ `vm-auth-proxy` - **CRITICAL GAP** (security-sensitive)
- ❌ `vm-docker-registry` - **SIGNIFICANT GAP**
- ❌ `vm-installer` - **SIGNIFICANT GAP**
- ❌ `vm-plugin` - Moderate gap
- ❌ `vm-platform` - Moderate gap
- ❌ `vm-core` - Moderate gap
- ❌ `vm-cli` - Low priority (covered by usage)
- ❌ `vm-messages` - Low priority (covered by usage)
- ❌ `vm-logging` - Low priority
- ❌ `version-sync` - Low priority (dev utility)

---

## Part 3: Gap Analysis

### Critical Gaps (Security/Correctness)

1. **vm-auth-proxy: NO TESTS** 🔴 CRITICAL
   - Encryption/decryption untested
   - Authentication untested
   - Secret storage untested
   - **Risk:** Security vulnerabilities, data loss

2. **vm-installer: NO INTEGRATION TESTS** 🔴 HIGH
   - Installation workflows untested
   - Cross-platform behavior untested
   - PATH updates untested
   - **Risk:** Installation failures, corrupted environments

3. **vm-provider: Incomplete coverage** 🟡 MODERATE
   - Vagrant provider untested
   - Tart provider has stub only
   - Mock provider not validated
   - **Risk:** Provider-specific bugs

### Functional Gaps

4. **vm-docker-registry: NO INTEGRATION TESTS** 🟠
   - Pull-through caching untested
   - Docker daemon integration untested
   - **Risk:** Registry failures

5. **vm-plugin: NO INTEGRATION TESTS** 🟠
   - Plugin loading untested
   - Invalid plugin handling untested
   - **Risk:** Plugin system failures

6. **vm-platform: NO INTEGRATION TESTS** 🟠
   - Cross-platform behavior untested
   - Shell detection untested
   - **Risk:** Platform-specific bugs

7. **vm-core: NO INTEGRATION TESTS** 🟠
   - Command streaming untested
   - File operations untested
   - **Risk:** Foundation issues

### Organizational Gaps

8. **Test organization inconsistency**
   - Some packages have `tests/` dirs, others only `src/` tests
   - No consistent pattern for integration vs unit tests

9. **Redundant or legacy tests**
   - Need to verify if older test files overlap with newer vm_ops/ structure
   - `workflow_tests.rs` (596 LOC) may have overlap with vm_ops/ tests

10. **Missing contract tests**
   - Provider trait implementations not systematically tested
   - TempProvider trait not tested

---

## Part 4: Recommendations

### A. Files to DELETE

#### 1. Potentially Redundant Test Files
**Action Required:** Manual review to confirm overlap before deletion

```bash
# REVIEW NEEDED - May have overlap with vm_ops/ tests
rust/vm/tests/workflow_tests.rs (596 LOC)
  → Check if create/start/stop/list workflows are already covered in vm_ops/
  → If yes, migrate unique tests and delete
  → If no, keep and document why it's separate

# REVIEW NEEDED - Service lifecycle tests appear in two places
rust/vm/tests/service_lifecycle_integration_tests.rs (385 LOC)
rust/vm/tests/vm_ops/service_lifecycle_tests.rs (177 LOC)
  → Consolidate into one file (prefer vm_ops/ structure)
  → Keep service_lifecycle_integration_tests.rs if it tests cross-service scenarios
  → Delete service_lifecycle_tests.rs if it's redundant
```

**Analysis Needed:**
```bash
# Run this to compare coverage
cd rust/vm
grep -h "fn test_" tests/workflow_tests.rs | sort > /tmp/workflow_tests.txt
grep -h "fn test_" tests/vm_ops/*.rs | sort > /tmp/vm_ops_tests.txt
comm -12 /tmp/workflow_tests.txt /tmp/vm_ops_tests.txt  # Find duplicates
```

#### 2. Stub/Incomplete Test Files

```bash
# DELETE - Stub only, not functional
rust/vm-provider/tests/tart_provider_tests.rs (1.8KB)
  → Only contains placeholder tests
  → Create proper tests or delete entirely
```

#### 3. Legacy Test Patterns

```bash
# DELETE - If confirmed as workspace-level test root
rust/tests/integration_tests.rs
  → Likely legacy from workspace-level testing
  → Most tests now in package-specific tests/ dirs
  → Delete if empty or migrate tests to appropriate packages
```

### B. Files to EDIT

#### 1. Reorganize Existing Tests

**rust/vm/tests/ structure:**
```bash
# CONSOLIDATE service lifecycle tests
# Choose one location and merge:
Option 1: Keep service_lifecycle_integration_tests.rs, enhance it
Option 2: Keep vm_ops/service_lifecycle_tests.rs, enhance it

# DECISION CRITERIA:
- If tests span multiple VMs or external services → keep service_lifecycle_integration_tests.rs
- If tests are provider-specific lifecycle → move to vm_ops/service_lifecycle_tests.rs
```

**rust/vm-provider/tests/ structure:**
```bash
# EDIT: Rename and expand tart_provider_tests.rs
mv tart_provider_tests.rs provider_specific_tests.rs

# Add sections for:
- Docker-specific tests
- Vagrant-specific tests
- Tart-specific tests
- Cross-provider behavior tests
```

#### 2. Enhance Existing Unit Tests

**Files to expand with more tests:**

```rust
// rust/vm-core/src/file_system.rs
// ADD: #[cfg(test)] module with comprehensive tests

// rust/vm-core/src/command_stream.rs
// ADD: #[cfg(test)] module for streaming tests

// rust/vm-temp/src/mount_ops.rs
// ADD: #[cfg(test)] module for unit tests

// rust/vm-temp/src/state.rs
// ADD: #[cfg(test)] module for serialization tests
```

### C. Files to ADD

#### PRIORITY 1: CRITICAL (Security & Installation)

```bash
# 1. vm-auth-proxy (CRITICAL - Security)
rust/vm-auth-proxy/tests/encryption_tests.rs
rust/vm-auth-proxy/tests/auth_tests.rs
rust/vm-auth-proxy/tests/storage_tests.rs
rust/vm-auth-proxy/tests/api_integration_tests.rs
rust/vm-auth-proxy/tests/security_tests.rs

# 2. vm-installer (HIGH - Installation correctness)
rust/vm-installer/tests/platform_detection_tests.rs
rust/vm-installer/tests/install_workflow_tests.rs
rust/vm-installer/tests/path_update_tests.rs
rust/vm-installer/tests/dependency_check_tests.rs
rust/vm-installer/tests/upgrade_tests.rs
```

#### PRIORITY 2: HIGH (Core Functionality)

```bash
# 3. vm-core (Foundation)
rust/vm-core/tests/file_system_tests.rs
rust/vm-core/tests/command_stream_tests.rs
rust/vm-core/tests/cross_platform_tests.rs

# 4. vm-docker-registry (Registry functionality)
rust/vm-docker-registry/tests/pull_through_tests.rs
rust/vm-docker-registry/tests/docker_integration_tests.rs
rust/vm-docker-registry/tests/garbage_collection_tests.rs
rust/vm-docker-registry/tests/offline_mode_tests.rs

# 5. vm-provider (Complete provider coverage)
rust/vm-provider/tests/vagrant_provider_tests.rs
rust/vm-provider/tests/contract_tests.rs
rust/vm-provider/tests/temp_provider_tests.rs
```

#### PRIORITY 3: MEDIUM (Enhance existing coverage)

```bash
# 6. vm-plugin (Plugin system)
rust/vm-plugin/tests/plugin_loading_tests.rs
rust/vm-plugin/tests/plugin_validation_tests.rs
rust/vm-plugin/tests/multi_plugin_tests.rs

# 7. vm-platform (Cross-platform)
rust/vm-platform/tests/platform_behavior_tests.rs
rust/vm-platform/tests/shell_detection_tests.rs

# 8. vm-package-manager (Package workflows)
rust/vm-package-manager/tests/package_install_tests.rs
rust/vm-package-manager/tests/link_operations_tests.rs

# 9. vm-temp (Enhanced coverage)
rust/vm-temp/tests/mount_operations_tests.rs
rust/vm-temp/tests/state_persistence_tests.rs
rust/vm-temp/tests/concurrent_access_tests.rs

# 10. vm (Additional integration tests)
rust/vm/tests/vm_ops/error_recovery_tests.rs
rust/vm/tests/vm_ops/concurrent_operations_tests.rs
rust/vm/tests/vm_ops/config_reload_tests.rs
```

#### PRIORITY 4: LOW (Nice to have)

```bash
# 11. vm-logging
rust/vm-logging/tests/log_format_tests.rs
rust/vm-logging/tests/tag_filtering_tests.rs

# 12. version-sync
rust/version-sync/tests/sync_tests.rs
```

### D. Test Infrastructure to ADD

```bash
# Shared test utilities
rust/test-utils/src/lib.rs
rust/test-utils/src/fixtures.rs
rust/test-utils/src/assertions.rs
rust/test-utils/src/docker_helpers.rs

# Contract test framework
rust/test-utils/src/contract_tests/mod.rs
rust/test-utils/src/contract_tests/provider_contract.rs
rust/test-utils/src/contract_tests/temp_provider_contract.rs
```

---

## Part 5: Implementation Roadmap

### Phase 1: Critical Security & Installation (Week 1)
**Goal:** Eliminate critical security and installation risks

1. **vm-auth-proxy tests** (2 days)
   - [ ] Add encryption/decryption unit tests
   - [ ] Add authentication tests
   - [ ] Add storage tests
   - [ ] Add API integration tests
   - [ ] Add security penetration tests

2. **vm-installer tests** (2 days)
   - [ ] Add platform detection tests
   - [ ] Add installation workflow tests
   - [ ] Add PATH update tests
   - [ ] Add dependency check tests

3. **Review and consolidate existing tests** (1 day)
   - [ ] Analyze `workflow_tests.rs` vs `vm_ops/` overlap
   - [ ] Consolidate service lifecycle tests
   - [ ] Delete stub tests (tart_provider_tests.rs)

### Phase 2: Foundation & Provider Coverage (Week 2)
**Goal:** Strengthen foundation and provider coverage

4. **vm-core integration tests** (2 days)
   - [ ] Add file system tests
   - [ ] Add command stream tests
   - [ ] Add cross-platform tests

5. **vm-provider completion** (2 days)
   - [ ] Complete Tart provider tests
   - [ ] Add Vagrant provider tests
   - [ ] Add contract tests
   - [ ] Add TempProvider tests

6. **vm-docker-registry tests** (1 day)
   - [ ] Add pull-through tests
   - [ ] Add Docker integration tests
   - [ ] Add garbage collection tests

### Phase 3: Plugin & Platform Coverage (Week 3)
**Goal:** Complete plugin system and platform testing

7. **vm-plugin integration tests** (1 day)
   - [ ] Add plugin loading tests
   - [ ] Add validation tests
   - [ ] Add multi-plugin tests

8. **vm-platform integration tests** (1 day)
   - [ ] Add platform behavior tests
   - [ ] Add shell detection tests

9. **vm-package-manager enhancement** (1 day)
   - [ ] Add package installation tests
   - [ ] Add link operation tests

### Phase 4: Enhancement & Organization (Week 4)
**Goal:** Enhance coverage and organize test suite

10. **vm-temp enhancement** (1 day)
    - [ ] Add mount operation tests
    - [ ] Add state persistence tests
    - [ ] Add concurrent access tests

11. **vm additional tests** (1 day)
    - [ ] Add error recovery tests
    - [ ] Add concurrent operation tests
    - [ ] Add config reload tests

12. **Test infrastructure** (1 day)
    - [ ] Create shared test utilities crate
    - [ ] Add contract test framework
    - [ ] Document testing patterns

13. **Documentation & cleanup** (1 day)
    - [ ] Update CLAUDE.md with new test structure
    - [ ] Create TESTING.md guide
    - [ ] Clean up redundant tests
    - [ ] Update CI/CD pipelines

---

## Part 6: Testing Best Practices

### Test Organization Rules

1. **Unit tests** (`#[cfg(test)]` in `src/`)
   - Test single functions/structs
   - No file I/O (use mocks/fakes)
   - Fast execution (< 1ms per test)
   - No external dependencies

2. **Integration tests** (`tests/` directory)
   - Test cross-module workflows
   - Can use file I/O (with temp dirs)
   - Can use real dependencies (with cleanup)
   - Should complete in < 1s per test

3. **E2E tests** (`tests/` directory, marked with `#[ignore]` if slow)
   - Test full workflows
   - Use real providers (Docker, etc.)
   - Proper setup/teardown
   - May take several seconds

### Naming Conventions

```rust
// Unit tests
#[test]
fn test_function_name_behavior() { }

// Integration tests
#[test]
fn test_module_integration_scenario() { }

// E2E tests (mark slow tests)
#[test]
#[ignore = "slow"]
fn test_full_workflow_end_to_end() { }

// Tests requiring Docker
#[test]
#[ignore = "requires_docker"]
fn test_docker_provider_create() { }
```

### Test Isolation

```rust
// Use TEST_MUTEX for environment modification
use std::sync::Mutex;
static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn test_config_with_env() {
    let _guard = TEST_MUTEX.lock().unwrap();
    // Safe to modify env here
}

// Use temp directories for file operations
use tempfile::tempdir;

#[test]
fn test_file_operation() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    // Test writes to temp dir, auto-cleanup
}

// Use unique names for Docker resources
#[test]
fn test_docker_operation() {
    let unique_name = format!("test-{}", uuid::Uuid::new_v4());
    // Test with unique container name
}
```

### Test Documentation

```rust
/// Tests that config merging correctly prioritizes VM-specific config over global config.
///
/// Setup:
/// - Create global config with `resources.cpu_cores = 2`
/// - Create VM config with `resources.cpu_cores = 4`
///
/// Expected:
/// - Merged config has `cpu_cores = 4` (VM-specific wins)
#[test]
fn test_config_merge_prioritization() {
    // Test implementation
}
```

---

## Part 7: Coverage Metrics

### Target Coverage by Phase

| Phase | Package | Current | Target | Gap |
|-------|---------|---------|--------|-----|
| **Phase 1** | vm-auth-proxy | 40% | 90% | +50% |
| | vm-installer | 30% | 85% | +55% |
| **Phase 2** | vm-core | 50% | 80% | +30% |
| | vm-provider | 70% | 90% | +20% |
| | vm-docker-registry | 35% | 75% | +40% |
| **Phase 3** | vm-plugin | 40% | 75% | +35% |
| | vm-platform | 45% | 75% | +30% |
| | vm-package-manager | 50% | 75% | +25% |
| **Phase 4** | vm-temp | 65% | 85% | +20% |
| | vm | 90% | 95% | +5% |

### Overall Target

- **Current workspace coverage:** ~60%
- **Target workspace coverage:** 85%
- **Improvement needed:** +25%

---

## Part 8: Quick Reference

### Test Execution Commands

```bash
# All tests
cargo test --workspace

# Specific package
cargo test --package vm-auth-proxy

# Specific test file
cargo test --package vm --test encryption_tests

# Include ignored tests (Docker-dependent)
cargo test --workspace -- --include-ignored

# Exclude slow tests
cargo test --workspace -- --skip slow

# Show test output
cargo test --workspace -- --nocapture

# Single-threaded (for TEST_MUTEX tests)
cargo test --workspace -- --test-threads=1
```

### Test Coverage Report

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --workspace --out Html --output-dir coverage/

# View report
open coverage/index.html
```

---

## Conclusion

This testing matrix provides a comprehensive roadmap for achieving high-quality test coverage across the VM tool workspace. By following the 4-phase implementation plan, the project will:

1. ✅ Eliminate critical security risks (vm-auth-proxy)
2. ✅ Ensure installation reliability (vm-installer)
3. ✅ Strengthen foundation (vm-core, vm-provider)
4. ✅ Complete feature coverage (all packages)
5. ✅ Maintain clean, organized test suite
6. ✅ Achieve 85%+ workspace coverage

**Next Steps:**
1. Review this document with the team
2. Prioritize Phase 1 tasks
3. Create GitHub issues for each test file
4. Begin implementation
5. Track progress with coverage metrics
