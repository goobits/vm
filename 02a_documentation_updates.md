# Documentation Updates

**Status:** Open

---

## DOC-001: Missing Docker Prerequisites

**Severity:** High (User Experience)
**Impact:** Installation fails, poor first impression

### Problem
- README doesn't mention Docker group/permissions requirement
- New users hit permission errors
- Forces users to discover workarounds themselves

### Current Error
```bash
./install.sh --build-from-source
# Error: permission denied while trying to connect to Docker daemon
```

### Checklist
- [ ] Update README.md Prerequisites section:
  ```markdown
  ## Prerequisites

  ### Required
  - **Docker** (with proper permissions)
    - Linux: Add user to docker group
      ```bash
      sudo usermod -aG docker $USER
      newgrp docker
      # Note: You may need to log out/in for changes to take effect
      ```
    - macOS: Install Docker Desktop
    - Windows: Install Docker Desktop with WSL2
  - **Rust toolchain** (1.70 or later)
    ```bash
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    ```
  - **Git**

  ### Verify Installation
  ```bash
  docker --version
  docker ps  # Should not error
  cargo --version
  ```
  ```
- [ ] Add troubleshooting section for common Docker issues
- [ ] Test instructions on fresh Ubuntu/Debian VM
- [ ] Update `vm doctor` command to check Docker permissions

### Files to Update
- `README.md`
- `rust/vm/src/commands/doctor.rs` (already improved in recent commits)

---

## DOC-002: Missing Development Tools Documentation

**Severity:** Medium
**Impact:** Inconsistent dev environments

### Problem
- Code quality tools not documented
- Developers can't run all checks locally
- Tools like `jscpd`, `rust-code-analysis-cli` undocumented

### Checklist
- [ ] Update CLAUDE.md with development tools section:
  ```markdown
  ## Development Tools

  ### Code Quality Analysis

  Install additional quality checking tools:

  ```bash
  # Code duplication detection
  npm install -g jscpd

  # Rust code complexity analysis
  cargo install rust-code-analysis-cli

  # Security scanning
  cargo install cargo-deny cargo-audit

  # Test coverage
  cargo install cargo-tarpaulin
  ```

  ### Running Quality Checks

  ```bash
  # Check for code duplication
  jscpd rust/ --threshold 2

  # Check for security vulnerabilities
  cd rust && cargo deny check
  cd rust && cargo audit

  # Check code complexity
  rust-code-analysis-cli --metrics -p rust/

  # Generate test coverage
  cd rust && cargo tarpaulin --workspace --out Html
  ```

  ### Pre-commit Checks

  The project uses pre-commit hooks for:
  - Rust formatting (`cargo fmt`)
  - Clippy linting
  - Quick tests for affected packages
  - Commit message validation

  See `.git/hooks/pre-commit` for details.
  ```
- [ ] Add CONTRIBUTING.md quality standards section
- [ ] Document code review process
- [ ] Add PR template with quality checklist

### Files to Update
- `CLAUDE.md`
- `CONTRIBUTING.md`
- `.github/PULL_REQUEST_TEMPLATE.md` (create)

---

## IMPROVE-002: Organize Test Files by Feature

**Severity:** Low
**Impact:** Improved maintainability, easier test discovery

### Problem
- 11 test files in `vm/tests/` with unclear organization
- Naming confusion (e.g., "service_lifecycle" means different things)

### Recommended Structure
```
vm/tests/
├── cli/
│   ├── config_commands.rs      # Config CLI tests
│   └── pkg_commands.rs          # Package CLI tests
├── services/
│   ├── service_manager.rs      # Service lifecycle
│   └── shared_services.rs      # Multi-VM service sharing
├── vm_operations/              # Already exists, keep as-is
│   ├── create_destroy_tests.rs
│   ├── lifecycle_tests.rs
│   └── ...
├── networking/
│   ├── port_forwarding.rs
│   └── ssh_refresh.rs
└── integration/
    └── provider_parity.rs
```

### Checklist
- [ ] Create new directory structure
- [ ] Move test files to appropriate directories
- [ ] Update test module declarations
- [ ] Update CLAUDE.md with new structure
- [ ] Verify all tests pass: `cargo test --package vm`
- [ ] Update TESTING_ACTION_PLAN.md references

### Files to Move
- `cli_integration_tests.rs` → `cli/config_commands.rs`
- `pkg_cli_tests.rs` → `cli/pkg_commands.rs`
- `service_lifecycle_integration_tests.rs` → `services/shared_services.rs`
- `port_forwarding_tests.rs` → `networking/port_forwarding.rs`
- `ssh_refresh.rs` → `networking/ssh_refresh.rs`

---

## Additional Documentation Tasks

### Quick Wins
- [ ] Add badges to README:
  ```markdown
  ![Build Status](badge-url)
  ![Coverage](badge-url)
  ![License](badge-url)
  ```
- [ ] Create SECURITY.md with vulnerability reporting process
- [ ] Add CHANGELOG.md automation (already exists, ensure up-to-date)
- [ ] Document new `vm db` commands in README
- [ ] Add examples/ directory with common use cases

### API Documentation
- [ ] Add module-level documentation to key crates
- [ ] Generate and publish rustdoc to GitHub Pages
- [ ] Add code examples to public APIs

---

## Success Criteria

- [ ] README clearly documents Docker prerequisites
- [ ] CLAUDE.md has complete dev tools section
- [ ] Test files organized by feature
- [ ] CONTRIBUTING.md has code quality guidelines
- [ ] All main docs have badges
- [ ] SECURITY.md exists with reporting process

---

## Benefits

- **User Experience:** Clear installation instructions
- **Contributor Experience:** Easy to get started developing
- **Discoverability:** Well-organized tests are easier to find
- **Standards:** Documented quality expectations
- **Professionalism:** Complete, polished documentation
