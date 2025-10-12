# Code Quality Improvements

**Priority:** Week 3-4
**Status:** Open
**Effort:** 3-5 days

---

## BUG-004: High Code Duplication (4.45%)

**Severity:** Medium (Maintainability)
**Impact:** Bug fixes must be applied in multiple places

### Problem
- 4.45% code duplication rate across Rust files (target: <2%)
- Primary duplication in test files
- Repeated fixture setup code (~8 files)
- Repeated container cleanup code (~5 files)

### Examples
```rust
// Duplicated in ~8 test files
struct WorkflowTestFixture {
    _temp_dir: TempDir,
    test_dir: PathBuf,
    binary_path: PathBuf,
}

// Duplicated in ~5 test files
fn cleanup_test_containers(&self) -> Result<()> {
    let _ = Command::new("docker")
        .args(["rm", "-f", &self.project_name])
        .output();
    Ok(())
}
```

### Checklist
- [ ] Run duplication analysis: `jscpd rust/`
- [ ] Create `rust/test-utils` crate with shared infrastructure
- [ ] Extract common patterns:
  - [ ] Test fixtures
  - [ ] Docker helpers
  - [ ] Assertions
  - [ ] Binary path resolution
  - [ ] Container cleanup utilities
- [ ] Update test files to use `test-utils`
- [ ] Verify all tests still pass: `cargo test --workspace`
- [ ] Re-run duplication check, target <2%

### Files Affected
- `rust/vm-provider/src/docker/lifecycle/creation.rs`
- `rust/vm-provider/src/docker/compose.rs`
- Test files in `rust/vm/tests/`, `rust/vm-config/tests/`

**Estimated LOC Reduction:** ~500 lines

---

## BUG-007: Format Args Linting Violations

**Severity:** Low (Code Style)
**Impact:** Code noisier than necessary, inconsistent with modern Rust idioms

### Problem
- `clippy::uninlined_format_args` violations in 100+ places
- Old style: `format!("{}", var)`
- New style: `format!("{var}")`

### Checklist
- [ ] Run per-crate fix:
  ```bash
  cd rust/vm-config && cargo clippy --fix --allow-dirty --allow-staged
  cd rust/vm-provider && cargo clippy --fix --allow-dirty --allow-staged
  cd rust/vm-package-server && cargo clippy --fix --allow-dirty --allow-staged
  cd rust/vm && cargo clippy --fix --allow-dirty --allow-staged
  ```
- [ ] Configure `clippy.toml` to enforce:
  ```toml
  disallowed-lints = ["clippy::uninlined_format_args"]
  ```
- [ ] Add clippy to CI with `-D warnings`
- [ ] Verify: `cargo clippy --workspace -- -D warnings`

### Files Affected
- Primarily `vm-config`, `vm-provider`, `vm-package-server`

---

## IMPROVE-001: Create Test Utilities Crate

**Priority:** High (Enables BUG-004 fix)
**Impact:** Reduces duplication, easier to write new tests

### Checklist
- [ ] Create crate structure:
  ```bash
  mkdir -p rust/test-utils/src
  touch rust/test-utils/Cargo.toml
  touch rust/test-utils/src/lib.rs
  touch rust/test-utils/src/fixtures.rs
  touch rust/test-utils/src/assertions.rs
  touch rust/test-utils/src/docker_helpers.rs
  ```
- [ ] Add to workspace: `rust/Cargo.toml`
- [ ] Implement shared utilities:
  - [ ] `TestFixture` - Standard test setup/teardown
  - [ ] `DockerHelpers` - Container management
  - [ ] `BinaryResolver` - Locate test binaries
  - [ ] `TempEnvironment` - Isolated test environments
  - [ ] Custom assertions for VM operations
- [ ] Add documentation and examples
- [ ] Migrate 3-5 test files as proof of concept
- [ ] Verify tests pass: `cargo test --workspace`

### Cargo.toml Template
```toml
[package]
name = "test-utils"
version.workspace = true
edition.workspace = true

[dependencies]
tempfile.workspace = true
uuid.workspace = true
anyhow.workspace = true
```

---

## Success Criteria

- [ ] Code duplication < 2%
- [ ] All clippy warnings fixed
- [ ] `test-utils` crate created and used in 5+ test files
- [ ] CI enforces clippy with `-D warnings`
- [ ] All tests pass: `cargo test --workspace`

---

## Estimated Timeline

- **BUG-004 + IMPROVE-001:** 3-4 days (create test-utils, refactor tests)
- **BUG-007:** 1 day (automated fixes)

**Total:** 4-5 days

---

## Benefits

- **DRY Principle:** Single source of truth for test patterns
- **Consistency:** All tests use same infrastructure
- **Velocity:** Faster to write new tests
- **Reliability:** Bug fixes apply everywhere automatically
- **Maintainability:** Easier onboarding for contributors
