# CI/CD & Automation Improvements

**Status:** Open

---

## CONFIG-001: Missing CI Coverage Enforcement

**Severity:** Low
**Impact:** No visibility into test coverage trends

### Problem
- No code coverage tracking in CI
- Coverage can decrease without detection
- No coverage badges or reports

### Checklist
- [ ] Install coverage tool:
  ```yaml
  - name: Install tarpaulin
    run: cargo install cargo-tarpaulin
  ```
- [ ] Add coverage job to CI:
  ```yaml
  - name: Generate coverage
    run: cargo tarpaulin --workspace --out Xml --output-dir coverage/

  - name: Upload coverage to Codecov
    uses: codecov/codecov-action@v3
    with:
      files: ./coverage/cobertura.xml
  ```
- [ ] Implement coverage trend checking:
  ```bash
  # scripts/check-coverage-trend.sh
  # Fail if coverage decreased by >2%
  ```
- [ ] Add coverage badge to README.md
- [ ] Set coverage targets:
  - [ ] Workspace: 85%
  - [ ] vm-auth-proxy: 90%
  - [ ] vm-installer: 85%
  - [ ] vm-core: 80%

### Files to Create/Update
- `.github/workflows/coverage.yml`
- `scripts/check-coverage-trend.sh`
- `README.md` (add badge)

---

## CONFIG-002: Clippy Lints Not Enforced in CI

**Severity:** Low
**Impact:** Style violations accumulate

### Problem
- Clippy runs locally but not enforced in CI
- Inconsistent code quality standards

### Checklist
- [ ] Add clippy job to CI:
  ```yaml
  - name: Run clippy
    run: cargo clippy --workspace --all-targets --all-features -- -D warnings
  ```
- [ ] Configure allowed lints in `clippy.toml`:
  ```toml
  # Allow list (if needed)
  allow = []

  # Deny list
  deny = [
    "clippy::uninlined_format_args",
    "clippy::redundant_clone",
    "clippy::unnecessary_wraps"
  ]
  ```
- [ ] Add to pre-commit hook (optional)
- [ ] Document clippy requirements in CONTRIBUTING.md
- [ ] Verify: `cargo clippy --workspace -- -D warnings` passes

### Files to Update
- `.github/workflows/ci.yml`
- `clippy.toml` (create if missing)
- `CONTRIBUTING.md`

---

## IMPROVE-003: Set Up Dependency Security Scanning

**Severity:** High (Security)
**Impact:** Automated vulnerability detection

### Problem
- Manual security checks only
- No automated scanning for CVEs
- Supply chain security not validated

### Checklist
- [ ] Fix `cargo deny` issues (see `01_critical_bugs.md` BUG-003)
- [ ] Add to CI:
  ```yaml
  security:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Install cargo-deny
        run: cargo install cargo-deny

      - name: Check security advisories
        run: cargo deny check advisories

      - name: Check licenses
        run: cargo deny check licenses
  ```
- [ ] Configure Dependabot:
  ```yaml
  # .github/dependabot.yml
  version: 2
  updates:
    - package-ecosystem: "cargo"
      directory: "/rust"
      schedule:
        interval: "weekly"
      open-pull-requests-limit: 10
  ```
- [ ] Set up security notifications
- [ ] Document security process in SECURITY.md

### Files to Create/Update
- `.github/workflows/security.yml`
- `.github/dependabot.yml` (create)
- `SECURITY.md` (create)
- `rust/deny.toml` (configure)

---

## Additional CI/CD Improvements

### Performance Benchmarking
- [ ] Add benchmark job (optional):
  ```yaml
  - name: Run benchmarks
    run: cargo bench --workspace
  ```

### Build Artifacts
- [ ] Cache cargo dependencies:
  ```yaml
  - uses: actions/cache@v3
    with:
      path: |
        ~/.cargo/registry
        ~/.cargo/git
        rust/target
      key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
  ```
- [ ] Build release artifacts for tags

### Documentation
- [ ] Auto-generate docs on push to main:
  ```yaml
  - name: Build docs
    run: cargo doc --workspace --no-deps

  - name: Deploy to GitHub Pages
    uses: peaceiris/actions-gh-pages@v3
    with:
      github_token: ${{ secrets.GITHUB_TOKEN }}
      publish_dir: ./rust/target/doc
  ```

---

## Success Criteria

- [ ] Coverage tracking active in CI
- [ ] Coverage badge on README shows current %
- [ ] Clippy enforced with `-D warnings`
- [ ] Security scanning runs on every PR
- [ ] Dependabot auto-creates update PRs
- [ ] Build caching reduces CI time by 30%+

---

## Benefits

- **Automation:** Catch issues before merge
- **Security:** Proactive vulnerability detection
- **Quality:** Enforce consistent standards
- **Visibility:** Coverage and quality metrics
- **Velocity:** Faster CI with caching
