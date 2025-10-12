# Testing Infrastructure Improvements

**Status:** Open

---

## BUG-005: Test Suite Parallel Execution Timeout

**Severity:** Medium
**Impact:** Slow test execution, poor developer experience

### Problem
- Test suite times out with default parallel execution
- Forced to use `--test-threads=1` (serial, slow)
- Resource contention from Docker containers running in parallel

### Checklist
- [ ] Identify resource-heavy tests: `cargo test --workspace -- --list`
- [ ] Split test execution strategy:
  - [ ] Unit tests: Run in parallel (fast)
  - [ ] Integration tests: Run serially or with limited parallelism
- [ ] Mark heavy tests with `#[ignore]` attribute
- [ ] Add resource limits to test containers
- [ ] Use Docker test mutex where appropriate
- [ ] Update CI to run tests in stages:
  ```yaml
  - name: Unit tests
    run: cargo test --workspace --lib
  - name: Integration tests
    run: cargo test --workspace --test '*' -- --test-threads=2
  ```
- [ ] Verify: `cargo test --workspace` completes < 10 minutes

### Files to Update
- Test files with Docker integration
- `.github/workflows/test.yml`
- `Makefile` test targets

---

## BUG-006: Integration Tests Skipped By Default

**Severity:** Medium
**Impact:** Incomplete test coverage visibility

### Problem
- `VM_INTEGRATION_TESTS=1` env var required
- Default `make test` skips integration tests
- Coverage numbers misleading

### Checklist
- [ ] Enable integration tests by default in CI
- [ ] Document when to skip: `SKIP_INTEGRATION_TESTS=1`
- [ ] Update test categorization:
  ```rust
  #[test]
  #[cfg_attr(feature = "integration", ignore)]
  fn test_that_needs_docker() { }
  ```
- [ ] Update documentation:
  - [ ] `CLAUDE.md` - Add integration test section
  - [ ] `README.md` - Add testing badge
- [ ] Verify CI runs all tests by default

---

## TEST-001: Missing Installation Workflow Tests

**Severity:** Medium
**Priority:** High (first user interaction)
**Package:** `vm-installer`

### Problem
- Zero integration tests for installation
- Installation is first user touchpoint

### Checklist
- [ ] Create test structure:
  ```bash
  mkdir -p rust/vm-installer/tests
  touch rust/vm-installer/tests/platform_detection_tests.rs
  touch rust/vm-installer/tests/install_workflow_tests.rs
  touch rust/vm-installer/tests/path_update_tests.rs
  touch rust/vm-installer/tests/dependency_check_tests.rs
  ```
- [ ] Add tests:
  - [ ] Platform detection (Linux, macOS, Windows)
  - [ ] Install from source workflow
  - [ ] PATH updates (.bashrc, .zshrc, .profile)
  - [ ] Upgrade existing installation
  - [ ] Docker dependency verification
- [ ] Target: 85%+ coverage
- [ ] Verify: `cargo test --package vm-installer`

---

## TEST-002: Missing Plugin System Tests

**Severity:** Medium
**Package:** `vm-plugin`

### Problem
- Limited integration tests for plugin loading/management

### Checklist
- [ ] Create test structure:
  ```bash
  mkdir -p rust/vm-plugin/tests
  mkdir -p rust/vm-plugin/tests/fixtures/plugins
  touch rust/vm-plugin/tests/plugin_loading_tests.rs
  touch rust/vm-plugin/tests/plugin_validation_tests.rs
  ```
- [ ] Create test fixtures:
  - [ ] Valid preset plugin
  - [ ] Valid service plugin
  - [ ] Invalid/malformed plugin
  - [ ] Conflicting plugin
- [ ] Add tests:
  - [ ] Plugin discovery
  - [ ] Plugin validation
  - [ ] Multi-plugin loading
  - [ ] Error handling
- [ ] Verify: `cargo test --package vm-plugin`

---

## TEST-003: Missing Docker Registry Tests

**Severity:** Medium
**Package:** `vm-docker-registry`

### Problem
- Zero integration tests for registry functionality

### Checklist
- [ ] Create test structure:
  ```bash
  mkdir -p rust/vm-docker-registry/tests
  touch rust/vm-docker-registry/tests/pull_through_tests.rs
  touch rust/vm-docker-registry/tests/docker_integration_tests.rs
  touch rust/vm-docker-registry/tests/garbage_collection_tests.rs
  ```
- [ ] Add tests:
  - [ ] Pull-through caching
  - [ ] Docker daemon integration
  - [ ] Garbage collection
  - [ ] Network failure recovery
- [ ] Verify: `cargo test --package vm-docker-registry`

---

## TEST-004: Missing Cross-Platform Tests

**Severity:** Medium
**Impact:** Platform compatibility validation

### Problem
- Tests primarily run on Linux only
- No cross-platform parity validation

### Checklist
- [ ] Set up GitHub Actions matrix:
  ```yaml
  strategy:
    matrix:
      os: [ubuntu-latest, macos-latest, windows-latest]
      rust: [stable]
    fail-fast: false
  ```
- [ ] Add platform-specific test markers:
  ```rust
  #[test]
  #[cfg(target_os = "linux")]
  fn test_linux_specific() { }
  ```
- [ ] Ensure Docker tests skip gracefully on platforms without Docker
- [ ] Validate provider behavior across platforms
- [ ] Test installation on all platforms

---

## Success Criteria

- [ ] Tests complete in parallel (< 10 minutes total)
- [ ] Integration tests run by default in CI
- [ ] vm-installer: 85%+ coverage
- [ ] vm-plugin: 80%+ coverage
- [ ] vm-docker-registry: 75%+ coverage
- [ ] CI runs on Linux, macOS, Windows

---

## Benefits

- **Reliability:** Catch bugs before production
- **Confidence:** Safe refactoring with comprehensive tests
- **Coverage:** Visibility into untested code paths
- **Platform Support:** Validated cross-platform compatibility
