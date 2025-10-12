# Testing Action Plan - Specific File Operations

**Date:** 2025-10-11
**Purpose:** Concrete actions to achieve clean, comprehensive test coverage

---

## 🔴 IMMEDIATE ACTIONS (Do First)

### 1. Analyze Potential Redundancy

**Before deleting anything, run this analysis:**

```bash
cd /workspace/rust/vm/tests

# Compare workflow_tests.rs with vm_ops/ tests
echo "=== workflow_tests.rs test functions ==="
grep "fn test_" workflow_tests.rs | sed 's/.*fn //' | sed 's/(.*//' | sort

echo -e "\n=== vm_ops/ test functions ==="
grep -r "fn test_" vm_ops/*.rs | sed 's/.*fn //' | sed 's/(.*//' | sort

# Check for duplicate test names
echo -e "\n=== Potential duplicates ==="
(grep "fn test_" workflow_tests.rs | sed 's/.*fn //' | sed 's/(.*//'; \
 grep -r "fn test_" vm_ops/*.rs | sed 's/.*fn //' | sed 's/(.*//' ) | \
 sort | uniq -d
```

### 2. Analyze Service Lifecycle Test Overlap

```bash
cd /workspace/rust/vm/tests

# Compare the two service lifecycle files
echo "=== service_lifecycle_integration_tests.rs (385 LOC) ==="
grep "fn test_" service_lifecycle_integration_tests.rs

echo -e "\n=== vm_ops/service_lifecycle_tests.rs (177 LOC) ==="
grep "fn test_" vm_ops/service_lifecycle_tests.rs
```

---

## ❌ FILES TO DELETE

### Confirmed Deletions

```bash
# 1. Stub test file (not functional, needs complete rewrite anyway)
rm rust/vm-provider/tests/tart_provider_tests.rs
```

### Conditional Deletions (After Analysis)

**Only delete after running the analysis above and confirming redundancy:**

```bash
# 2. If workflow_tests.rs is fully covered by vm_ops/
# CONDITION: All unique tests have been migrated to vm_ops/
# ACTION: Review line-by-line, migrate unique tests, then:
# rm rust/vm/tests/workflow_tests.rs

# 3. If service lifecycle tests are redundant
# CONDITION: One file subsumes the other completely
# ACTION: Consolidate into the more comprehensive file, then:
# rm rust/vm/tests/vm_ops/service_lifecycle_tests.rs
# OR
# rm rust/vm/tests/service_lifecycle_integration_tests.rs

# 4. Workspace-level test file (if empty or redundant)
# CONDITION: Check if it contains any unique tests
# ACTION: Migrate unique tests to appropriate packages, then:
# rm rust/tests/integration_tests.rs
```

**Decision Tree for Deletion:**

```
workflow_tests.rs (596 LOC)
├─ Contains unique multi-command workflows? → KEEP
├─ All tests covered in vm_ops/? → DELETE
└─ Mix of unique and redundant? → MIGRATE unique tests, then DELETE

service_lifecycle_integration_tests.rs (385 LOC) vs vm_ops/service_lifecycle_tests.rs (177 LOC)
├─ integration_tests covers multi-VM scenarios? → KEEP integration_tests, DELETE vm_ops version
├─ vm_ops version covers provider-specific tests? → KEEP vm_ops, DELETE integration_tests
└─ Both have unique tests? → MERGE into single file (prefer vm_ops location)

integration_tests.rs (workspace root)
├─ Empty or trivial? → DELETE
├─ Cross-crate integration tests? → KEEP (but document purpose)
└─ Package-specific tests? → MIGRATE to package, then DELETE
```

---

## ✏️  FILES TO EDIT (Enhance Existing)

### High Priority Edits

#### 1. vm-provider: Reorganize provider tests

```bash
# Current: rust/vm-provider/tests/tart_provider_tests.rs (stub)
# Action: Delete and replace with comprehensive provider tests

# Create: rust/vm-provider/tests/provider_specific_tests.rs
```

**Content structure:**
```rust
// provider_specific_tests.rs
//! Provider-specific behavior tests
//!
//! Tests unique behaviors and edge cases for each provider implementation.

#[cfg(test)]
mod docker_specific {
    // Docker-specific tests
}

#[cfg(test)]
mod vagrant_specific {
    // Vagrant-specific tests (if feature enabled)
}

#[cfg(test)]
mod tart_specific {
    // Tart-specific tests (if feature enabled)
}
```

#### 2. vm-core: Add integration test file

```bash
# Create: rust/vm-core/tests/integration_tests.rs
```

**Content structure:**
```rust
// integration_tests.rs
//! Integration tests for vm-core utilities
//!
//! Tests cross-module interactions and real file system operations.

mod command_stream_tests;
mod file_system_tests;
mod cross_platform_tests;
```

#### 3. vm/tests: Consolidate test structure

**Option A: Keep workflow_tests.rs (if unique workflows)**
```rust
// Edit workflow_tests.rs
// Add documentation explaining what makes these tests different from vm_ops/
//! Multi-command workflow tests
//!
//! These tests verify complex command sequences and state transitions
//! across multiple VMs, unlike vm_ops/ which tests individual commands.

// Keep only truly unique multi-command workflows
```

**Option B: Delete workflow_tests.rs (if redundant)**
```bash
# Migrate unique tests to appropriate vm_ops/ files
# Then delete workflow_tests.rs
```

#### 4. Enhance existing unit tests with #[cfg(test)] modules

```rust
// Edit: rust/vm-core/src/file_system.rs
// Add at end of file:

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_copy_dir() {
        let src_dir = tempdir().unwrap();
        let dst_dir = tempdir().unwrap();
        // Add tests
    }
}

// Similar edits for:
// - rust/vm-core/src/command_stream.rs
// - rust/vm-temp/src/mount_ops.rs
// - rust/vm-temp/src/state.rs
```

---

## ✅ FILES TO ADD

### Phase 1: CRITICAL (Security & Installation)

#### vm-auth-proxy (CRITICAL - Zero tests currently)

```bash
mkdir -p rust/vm-auth-proxy/tests

# Create 5 test files:
touch rust/vm-auth-proxy/tests/encryption_tests.rs
touch rust/vm-auth-proxy/tests/auth_tests.rs
touch rust/vm-auth-proxy/tests/storage_tests.rs
touch rust/vm-auth-proxy/tests/api_integration_tests.rs
touch rust/vm-auth-proxy/tests/security_tests.rs

# Create common test utilities
touch rust/vm-auth-proxy/tests/common/mod.rs
```

**Test file templates:**

```rust
// encryption_tests.rs
//! Encryption and decryption tests for AES-256-GCM

use vm_auth_proxy::crypto;

#[test]
fn test_encrypt_decrypt_round_trip() {
    let plaintext = b"secret_api_key_12345";
    let passphrase = "test_passphrase";

    let encrypted = crypto::encrypt(plaintext, passphrase).unwrap();
    let decrypted = crypto::decrypt(&encrypted, passphrase).unwrap();

    assert_eq!(plaintext, decrypted.as_slice());
}

#[test]
fn test_wrong_passphrase_fails() {
    let plaintext = b"secret";
    let encrypted = crypto::encrypt(plaintext, "correct").unwrap();

    let result = crypto::decrypt(&encrypted, "wrong");
    assert!(result.is_err());
}

#[test]
fn test_corrupted_ciphertext_fails() {
    let plaintext = b"secret";
    let mut encrypted = crypto::encrypt(plaintext, "pass").unwrap();

    // Corrupt the ciphertext
    encrypted[0] ^= 0xFF;

    let result = crypto::decrypt(&encrypted, "pass");
    assert!(result.is_err());
}

// Add more tests:
// - test_key_derivation_pbkdf2
// - test_salt_randomness
// - test_nonce_uniqueness
// - test_empty_plaintext
// - test_large_plaintext (>1MB)
```

```rust
// auth_tests.rs
//! Authentication and authorization tests

use vm_auth_proxy::server;

#[test]
fn test_bearer_token_validation() {
    // Test valid token
    // Test invalid token
    // Test expired token
    // Test missing token
}

#[test]
fn test_token_generation() {
    // Test token uniqueness
    // Test token format
}

// Add more tests:
// - test_concurrent_authentication
// - test_rate_limiting
// - test_session_management
```

```rust
// storage_tests.rs
//! Secret storage tests

use vm_auth_proxy::storage;
use tempfile::tempdir;

#[test]
fn test_store_and_retrieve_secret() {
    let dir = tempdir().unwrap();
    let storage = storage::SecretStorage::new(dir.path()).unwrap();

    storage.store("key1", "value1").unwrap();
    let value = storage.retrieve("key1").unwrap();

    assert_eq!(value, "value1");
}

#[test]
fn test_delete_secret() {
    let dir = tempdir().unwrap();
    let storage = storage::SecretStorage::new(dir.path()).unwrap();

    storage.store("key1", "value1").unwrap();
    storage.delete("key1").unwrap();

    assert!(storage.retrieve("key1").is_err());
}

// Add more tests:
// - test_list_secrets
// - test_overwrite_secret
// - test_concurrent_access
// - test_storage_persistence
// - test_storage_limits
```

```rust
// api_integration_tests.rs
//! HTTP API integration tests

use axum::http::StatusCode;
use vm_auth_proxy::server;

#[tokio::test]
async fn test_store_secret_api() {
    let app = server::app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/secrets/key1")
                .method("POST")
                .header("Authorization", "Bearer valid_token")
                .body(Body::from("secret_value"))
                .unwrap()
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
}

// Add more tests:
// - test_retrieve_secret_api
// - test_delete_secret_api
// - test_list_secrets_api
// - test_unauthorized_access
// - test_invalid_requests
```

```rust
// security_tests.rs
//! Security and penetration tests

use vm_auth_proxy::{server, crypto};

#[test]
fn test_timing_attack_resistance() {
    // Test that authentication takes similar time for valid/invalid tokens
}

#[test]
fn test_sql_injection_prevention() {
    // Test that malicious input doesn't cause SQL injection
}

#[test]
fn test_path_traversal_prevention() {
    // Test that secret keys can't cause path traversal
}

// Add more tests:
// - test_xss_prevention
// - test_csrf_protection
// - test_encryption_strength
// - test_key_rotation
```

#### vm-installer (HIGH - Zero integration tests currently)

```bash
mkdir -p rust/vm-installer/tests

# Create test files:
touch rust/vm-installer/tests/platform_detection_tests.rs
touch rust/vm-installer/tests/install_workflow_tests.rs
touch rust/vm-installer/tests/path_update_tests.rs
touch rust/vm-installer/tests/dependency_check_tests.rs
touch rust/vm-installer/tests/upgrade_tests.rs

# Create common utilities
touch rust/vm-installer/tests/common/mod.rs
```

**Test file templates:**

```rust
// platform_detection_tests.rs
//! Platform and architecture detection tests

use vm_installer::platform;

#[test]
fn test_detect_linux_x86_64() {
    // Test Linux detection
}

#[test]
fn test_detect_macos_aarch64() {
    // Test macOS ARM detection
}

// Add tests for:
// - Windows detection
// - Cross-compilation target detection
// - Unsupported platform handling
```

```rust
// install_workflow_tests.rs
//! Installation workflow integration tests

use vm_installer::installer;
use tempfile::tempdir;

#[test]
fn test_install_from_source() {
    let install_dir = tempdir().unwrap();

    // Test full installation workflow
    // - Build from source
    // - Copy binary
    // - Update PATH
    // - Verify installation
}

#[test]
fn test_install_overwrites_existing() {
    // Test upgrade scenario
}

// Add tests for:
// - First-time installation
// - Installation with existing binary
// - Installation without cargo
// - Installation permission errors
```

```rust
// path_update_tests.rs
//! PATH and shell configuration update tests

use vm_installer::installer;
use tempfile::tempdir;

#[test]
fn test_update_bashrc() {
    let home = tempdir().unwrap();
    let bashrc = home.path().join(".bashrc");

    // Test PATH addition to .bashrc
}

#[test]
fn test_update_zshrc() {
    // Test PATH addition to .zshrc
}

// Add tests for:
// - .profile updates
// - fish shell config
// - PowerShell profile (Windows)
// - Idempotent updates (don't add duplicate PATHs)
```

```rust
// dependency_check_tests.rs
//! Dependency verification tests

use vm_installer::dependencies;

#[test]
fn test_check_cargo_installed() {
    // Test cargo detection
}

#[test]
fn test_check_docker_installed() {
    // Test Docker detection
}

// Add tests for:
// - rustc version check
// - Docker version check
// - Missing dependency reporting
```

```rust
// upgrade_tests.rs
//! Upgrade and migration tests

use vm_installer::installer;

#[test]
fn test_upgrade_from_v2_to_v2_2() {
    // Test version upgrade
}

#[test]
fn test_downgrade_prevention() {
    // Test that downgrades warn or fail
}

// Add tests for:
// - Migration of config files
// - Backup creation before upgrade
// - Rollback on failure
```

### Phase 2: HIGH (Foundation & Providers)

#### vm-core

```bash
mkdir -p rust/vm-core/tests

touch rust/vm-core/tests/file_system_tests.rs
touch rust/vm-core/tests/command_stream_tests.rs
touch rust/vm-core/tests/cross_platform_tests.rs
touch rust/vm-core/tests/integration_tests.rs
```

**Key tests to add:**

```rust
// file_system_tests.rs
#[test]
fn test_copy_directory_recursive() { }

#[test]
fn test_symlink_handling() { }

#[test]
fn test_permission_preservation() { }
```

```rust
// command_stream_tests.rs
#[test]
fn test_stream_stdout_realtime() { }

#[test]
fn test_handle_command_failure() { }

#[test]
fn test_concurrent_command_streams() { }
```

```rust
// cross_platform_tests.rs
#[test]
fn test_path_resolution_linux() { }

#[test]
fn test_path_resolution_macos() { }

#[test]
fn test_path_resolution_windows() { }
```

#### vm-docker-registry

```bash
mkdir -p rust/vm-docker-registry/tests

touch rust/vm-docker-registry/tests/pull_through_tests.rs
touch rust/vm-docker-registry/tests/docker_integration_tests.rs
touch rust/vm-docker-registry/tests/garbage_collection_tests.rs
touch rust/vm-docker-registry/tests/offline_mode_tests.rs
```

#### vm-provider

```bash
# Enhance existing tests
touch rust/vm-provider/tests/vagrant_provider_tests.rs
touch rust/vm-provider/tests/contract_tests.rs
touch rust/vm-provider/tests/temp_provider_tests.rs
touch rust/vm-provider/tests/provider_specific_tests.rs
```

**Key contract tests:**

```rust
// contract_tests.rs
//! Contract tests for Provider trait implementations

/// Test that all providers implement the Provider trait correctly
#[test]
fn test_provider_trait_contract() {
    let providers = vec![
        Box::new(DockerProvider::new(config.clone()).unwrap()) as Box<dyn Provider>,
        // Add other providers
    ];

    for provider in providers {
        // Test that all trait methods work
        assert!(provider.name().len() > 0);
        assert!(provider.supports_multi_instance() == true || provider.supports_multi_instance() == false);
        // Test other trait methods
    }
}

/// Test that create() → start() → stop() → destroy() works for all providers
#[test]
#[ignore = "requires_docker"]
fn test_lifecycle_contract_all_providers() {
    // Test full lifecycle for each provider
}
```

### Phase 3: MEDIUM (Plugin & Platform)

#### vm-plugin

```bash
mkdir -p rust/vm-plugin/tests

touch rust/vm-plugin/tests/plugin_loading_tests.rs
touch rust/vm-plugin/tests/plugin_validation_tests.rs
touch rust/vm-plugin/tests/multi_plugin_tests.rs

mkdir -p rust/vm-plugin/tests/fixtures/plugins
```

**Test fixture structure:**

```bash
rust/vm-plugin/tests/fixtures/
├── valid_preset_plugin/
│   ├── vm.yaml
│   └── metadata.yaml
├── valid_service_plugin/
│   ├── docker-compose.yml
│   └── metadata.yaml
├── invalid_plugin/
│   └── metadata.yaml (malformed)
└── conflicting_plugin/
    └── vm.yaml (conflicts with another plugin)
```

#### vm-platform

```bash
mkdir -p rust/vm-platform/tests

touch rust/vm-platform/tests/platform_behavior_tests.rs
touch rust/vm-platform/tests/shell_detection_tests.rs
touch rust/vm-platform/tests/resource_detection_tests.rs
```

#### vm-package-manager

```bash
# Add to existing tests/
cd rust/vm-package-manager/tests

touch package_install_tests.rs
touch link_operations_tests.rs
```

### Phase 4: LOW (Enhancement & Organization)

#### vm-temp

```bash
cd rust/vm-temp/tests

touch mount_operations_tests.rs
touch state_persistence_tests.rs
touch concurrent_access_tests.rs
```

#### vm (additional tests)

```bash
cd rust/vm/tests/vm_ops

touch error_recovery_tests.rs
touch concurrent_operations_tests.rs
touch config_reload_tests.rs
```

#### Test infrastructure

```bash
# Create shared test utilities crate
mkdir -p rust/test-utils/src

touch rust/test-utils/Cargo.toml
touch rust/test-utils/src/lib.rs
touch rust/test-utils/src/fixtures.rs
touch rust/test-utils/src/assertions.rs
touch rust/test-utils/src/docker_helpers.rs
touch rust/test-utils/src/temp_helpers.rs

mkdir -p rust/test-utils/src/contract_tests
touch rust/test-utils/src/contract_tests/mod.rs
touch rust/test-utils/src/contract_tests/provider_contract.rs
touch rust/test-utils/src/contract_tests/temp_provider_contract.rs
```

**rust/test-utils/Cargo.toml:**

```toml
[package]
name = "test-utils"
version.workspace = true
edition.workspace = true

[dependencies]
tempfile.workspace = true
uuid.workspace = true
anyhow.workspace = true

[dev-dependencies]
```

**rust/test-utils/src/lib.rs:**

```rust
//! Shared test utilities for the VM tool workspace
//!
//! This crate provides common test helpers, fixtures, and assertions
//! used across multiple test suites.

pub mod assertions;
pub mod contract_tests;
pub mod docker_helpers;
pub mod fixtures;
pub mod temp_helpers;

// Re-export commonly used items
pub use assertions::*;
pub use fixtures::*;
pub use temp_helpers::*;
```

---

## 📋 EXECUTION CHECKLIST

### Pre-Execution Checklist

- [ ] Run redundancy analysis (commands in section 1)
- [ ] Back up current test files: `cp -r rust/ rust-backup-$(date +%Y%m%d)/`
- [ ] Create git branch: `git checkout -b test-matrix-cleanup`
- [ ] Document current coverage: `cargo tarpaulin --workspace --out Html`

### Phase 1: Critical (Week 1)

**Day 1-2: vm-auth-proxy**
- [ ] Create `rust/vm-auth-proxy/tests/` directory
- [ ] Add `encryption_tests.rs`
- [ ] Add `auth_tests.rs`
- [ ] Add `storage_tests.rs`
- [ ] Run tests: `cargo test --package vm-auth-proxy`
- [ ] Verify 90%+ coverage for crypto module

**Day 3-4: vm-installer**
- [ ] Create `rust/vm-installer/tests/` directory
- [ ] Add `platform_detection_tests.rs`
- [ ] Add `install_workflow_tests.rs`
- [ ] Add `path_update_tests.rs`
- [ ] Add `dependency_check_tests.rs`
- [ ] Run tests: `cargo test --package vm-installer`
- [ ] Verify installation workflows work cross-platform

**Day 5: Consolidation**
- [ ] Run analysis scripts for redundancy detection
- [ ] Delete `tart_provider_tests.rs` (confirmed stub)
- [ ] Migrate unique tests from `workflow_tests.rs` if needed
- [ ] Delete or consolidate service lifecycle tests
- [ ] Update `CLAUDE.md` with changes
- [ ] Run full test suite: `cargo test --workspace`

### Phase 2: Foundation (Week 2)

**Day 6-7: vm-core**
- [ ] Create `rust/vm-core/tests/` directory
- [ ] Add `file_system_tests.rs`
- [ ] Add `command_stream_tests.rs`
- [ ] Add `cross_platform_tests.rs`
- [ ] Run tests: `cargo test --package vm-core`

**Day 8-9: vm-provider**
- [ ] Delete `tart_provider_tests.rs`
- [ ] Add `provider_specific_tests.rs`
- [ ] Add `vagrant_provider_tests.rs`
- [ ] Add `contract_tests.rs`
- [ ] Add `temp_provider_tests.rs`
- [ ] Run tests: `cargo test --package vm-provider`

**Day 10: vm-docker-registry**
- [ ] Create `rust/vm-docker-registry/tests/` directory
- [ ] Add `pull_through_tests.rs`
- [ ] Add `docker_integration_tests.rs`
- [ ] Add `garbage_collection_tests.rs`
- [ ] Run tests: `cargo test --package vm-docker-registry`

### Phase 3: Plugin & Platform (Week 3)

**Day 11: vm-plugin**
- [ ] Create `rust/vm-plugin/tests/` directory
- [ ] Add `plugin_loading_tests.rs`
- [ ] Add `plugin_validation_tests.rs`
- [ ] Add `multi_plugin_tests.rs`
- [ ] Create test fixtures
- [ ] Run tests: `cargo test --package vm-plugin`

**Day 12: vm-platform**
- [ ] Create `rust/vm-platform/tests/` directory
- [ ] Add `platform_behavior_tests.rs`
- [ ] Add `shell_detection_tests.rs`
- [ ] Run tests: `cargo test --package vm-platform`

**Day 13: vm-package-manager**
- [ ] Add `package_install_tests.rs`
- [ ] Add `link_operations_tests.rs`
- [ ] Run tests: `cargo test --package vm-package-manager`

### Phase 4: Enhancement (Week 4)

**Day 14: vm-temp**
- [ ] Add `mount_operations_tests.rs`
- [ ] Add `state_persistence_tests.rs`
- [ ] Add `concurrent_access_tests.rs`
- [ ] Run tests: `cargo test --package vm-temp`

**Day 15: vm additional tests**
- [ ] Add `vm_ops/error_recovery_tests.rs`
- [ ] Add `vm_ops/concurrent_operations_tests.rs`
- [ ] Add `vm_ops/config_reload_tests.rs`
- [ ] Run tests: `cargo test --package vm`

**Day 16: Test infrastructure**
- [ ] Create `rust/test-utils/` crate
- [ ] Add shared utilities
- [ ] Add contract test framework
- [ ] Update other tests to use test-utils
- [ ] Run tests: `cargo test --workspace`

**Day 17: Documentation & CI**
- [ ] Update `CLAUDE.md` with complete test structure
- [ ] Create `TESTING.md` guide
- [ ] Update CI/CD pipeline
- [ ] Generate coverage report
- [ ] Verify 85%+ workspace coverage

### Post-Execution Checklist

- [ ] All tests pass: `cargo test --workspace`
- [ ] Coverage target met: `cargo tarpaulin --workspace`
- [ ] Documentation updated
- [ ] Git commit: `git commit -m "feat: comprehensive test coverage refactor"`
- [ ] Create PR for review
- [ ] Delete backup: `rm -rf rust-backup-*` (after PR merged)

---

## 🎯 SUCCESS CRITERIA

### Quantitative Metrics

- [ ] Workspace coverage: **85%+** (currently ~60%)
- [ ] vm-auth-proxy coverage: **90%+** (currently ~40%)
- [ ] vm-installer coverage: **85%+** (currently ~30%)
- [ ] vm-provider coverage: **90%+** (currently ~70%)
- [ ] vm-core coverage: **80%+** (currently ~50%)
- [ ] All critical packages: **80%+** coverage

### Qualitative Metrics

- [ ] Zero redundant test files
- [ ] Consistent test organization across packages
- [ ] All providers have contract tests
- [ ] All security-sensitive code has tests
- [ ] All CLI commands have E2E tests
- [ ] Clear test documentation

### CI/CD Metrics

- [ ] Test suite completes in < 10 minutes
- [ ] No flaky tests (random failures)
- [ ] All Docker-dependent tests properly marked with `#[ignore]`
- [ ] All tests use proper isolation (TEST_MUTEX, temp dirs, etc.)

---

## 📚 RESOURCES

### Documentation to Update

1. **CLAUDE.md** - Update "Running Tests" section
2. **New file: TESTING.md** - Comprehensive testing guide
3. **README.md** - Add testing badge and quick start

### CI/CD Updates

```yaml
# .github/workflows/test.yml
name: Test Suite

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
      - name: Run tests
        run: cargo test --workspace
      - name: Run coverage
        run: |
          cargo install cargo-tarpaulin
          cargo tarpaulin --workspace --out Xml
      - name: Upload coverage
        uses: codecov/codecov-action@v3
```

### Commands Reference

```bash
# Run specific phase tests
cargo test --package vm-auth-proxy        # Phase 1
cargo test --package vm-installer         # Phase 1
cargo test --package vm-core              # Phase 2
cargo test --package vm-provider          # Phase 2
cargo test --package vm-docker-registry   # Phase 2
cargo test --package vm-plugin            # Phase 3
cargo test --package vm-platform          # Phase 3
cargo test --package vm-package-manager   # Phase 3
cargo test --package vm-temp              # Phase 4
cargo test --package vm --test vm_ops     # Phase 4

# Coverage tracking
cargo tarpaulin --package vm-auth-proxy --out Html
cargo tarpaulin --workspace --out Html --output-dir coverage/

# Benchmark test execution time
time cargo test --workspace --release
```

---

## 🚨 RISK MITIGATION

### Backup Strategy

```bash
# Before starting Phase 1
cd /workspace
tar -czf vm-tests-backup-$(date +%Y%m%d).tar.gz rust/*/tests/

# Store backup
mv vm-tests-backup-*.tar.gz ~/backups/
```

### Rollback Plan

If tests break existing functionality:

1. **Identify the breaking change:**
   ```bash
   git log --oneline --all --graph
   git diff HEAD~1
   ```

2. **Revert specific file:**
   ```bash
   git checkout HEAD~1 -- rust/vm-auth-proxy/tests/encryption_tests.rs
   ```

3. **Full rollback:**
   ```bash
   git reset --hard origin/main
   ```

### Incremental Validation

After each phase:

```bash
# 1. Run all tests
cargo test --workspace

# 2. Check for compilation errors
cargo check --workspace

# 3. Run clippy
cargo clippy --workspace

# 4. Check formatting
cargo fmt --check

# 5. Generate coverage
cargo tarpaulin --workspace --out Html

# 6. Commit progress
git add -A
git commit -m "test: complete phase X - [package-name]"
```

---

## 🎓 TESTING PATTERNS TO FOLLOW

### Pattern 1: Isolated Test Setup

```rust
#[test]
fn test_example() {
    // Use unique names to avoid conflicts
    let test_id = uuid::Uuid::new_v4();
    let container_name = format!("test-container-{}", test_id);

    // Use temp directories
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    // Test code here

    // Cleanup happens automatically (Drop trait)
}
```

### Pattern 2: Environment Isolation

```rust
use std::sync::Mutex;
static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn test_with_env() {
    let _guard = TEST_MUTEX.lock().unwrap();

    // Modify environment safely
    std::env::set_var("TEST_VAR", "value");

    // Test code

    // Cleanup
    std::env::remove_var("TEST_VAR");
}
```

### Pattern 3: Async Test with Timeout

```rust
#[tokio::test(flavor = "multi_thread")]
#[timeout(Duration::from_secs(30))]
async fn test_async_operation() {
    // Async test code
}
```

### Pattern 4: Conditional Test Execution

```rust
#[test]
#[cfg_attr(not(feature = "docker"), ignore = "requires Docker")]
fn test_docker_operation() {
    // Docker-specific test
}

#[test]
#[cfg(target_os = "linux")]
fn test_linux_specific() {
    // Linux-only test
}
```

### Pattern 5: Parameterized Tests

```rust
#[test]
fn test_multiple_inputs() {
    let test_cases = vec![
        ("input1", "expected1"),
        ("input2", "expected2"),
        ("input3", "expected3"),
    ];

    for (input, expected) in test_cases {
        let result = function_under_test(input);
        assert_eq!(result, expected, "Failed for input: {}", input);
    }
}
```

---

## ✅ FINAL VALIDATION

Before marking complete, verify:

```bash
# 1. All tests pass
cargo test --workspace --all-features

# 2. No warnings
cargo clippy --workspace -- -D warnings

# 3. Code is formatted
cargo fmt --check

# 4. Coverage meets targets
cargo tarpaulin --workspace --out Html
# Open coverage/index.html and verify 85%+ overall

# 5. Documentation builds
cargo doc --workspace --no-deps

# 6. No dead code
cargo clippy --workspace -- -D dead_code

# 7. Dependencies are clean
cargo machete

# 8. Security audit passes
cargo audit
```

---

**End of Action Plan**

This document provides concrete, actionable steps to achieve comprehensive test coverage while maintaining a clean codebase with zero legacy code.
