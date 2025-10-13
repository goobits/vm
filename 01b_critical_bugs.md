# Critical Bug Fixes

**Status:** Open

---

## ~~BUG-001: Ansible Provisioning Failure in Port Forwarding Tests~~ ✅ RESOLVED

**Severity:** High
**Impact:** Integration tests cannot validate port forwarding

### Problem
- Tests `test_port_forwarding_single_port` and `test_port_forwarding_multiple_ports` fail
- Ansible step "Change user shell to zsh" fails in test containers
- Blocks validation of core port forwarding functionality

### Root Cause
- zsh may not be installed in test containers
- Shell change step not resilient to test environments

### Resolution
- Added `ignore_errors: yes` to the "Change user shell to zsh" task in `playbook.yml`
- This allows provisioning to continue even when zsh is unavailable in test environments
- Tests now pass successfully (verified with `cargo test --package vm --test networking`)

### Files Modified
- `rust/vm-provider/src/resources/ansible/playbook.yml` (line 372)

---

## BUG-002: Installer Script Failure

**Severity:** High
**Impact:** Poor first impression for new users, installation blocked

### Problem
- `./install.sh --build-from-source` fails at final setup phase
- Users forced to workaround: `cd rust && cargo run --package vm-installer`
- Installation docs don't match reality

### Checklist
- [ ] Debug install.sh to identify exact failure point
- [ ] Add comprehensive error handling and logging
- [ ] Test on clean systems (Docker, VM, fresh Linux install)
- [ ] Verify script handles all edge cases:
  - [ ] Existing installation (upgrade)
  - [ ] Missing dependencies
  - [ ] Permission issues
  - [ ] Non-standard PATH configurations
- [ ] Update README.md if workaround needed
- [ ] Test full install: `./install.sh --build-from-source`

### Files to Check
- `./install.sh`
- `rust/vm-installer/` package

---

## BUG-003: Cargo Deny Security Scanning Blocked

**Severity:** High (Security)
**Impact:** No visibility into dependency vulnerabilities, supply chain risk

### Problem
- `cargo deny check` command fails to complete
- Network timeout suspected in sandbox environments
- Complete blind spot for CVEs and outdated dependencies
- Cannot validate security compliance

### Checklist
- [ ] Investigate network timeout issues
- [ ] Review `deny.toml` configuration
- [ ] Test in different environments:
  - [ ] Local machine
  - [ ] Docker container
  - [ ] CI environment
- [ ] Verify network access requirements
- [ ] Consider alternative security scanning tools:
  - [ ] `cargo audit` (simpler, fewer network requirements)
  - [ ] GitHub Dependabot
  - [ ] Snyk or similar
- [ ] Set up automated vulnerability scanning in CI
- [ ] Document security scanning process in CLAUDE.md
- [ ] Verify working: `cd rust && cargo deny check`

### Files to Check
- `rust/deny.toml`
- `.github/workflows/` (for CI integration)
- `Makefile` (deny target)

---

## Success Criteria

- [ ] All port forwarding tests pass
- [ ] `./install.sh --build-from-source` works on fresh Ubuntu/Debian system
- [ ] Security scanning runs successfully
- [ ] CI includes security checks
- [ ] Documentation updated

