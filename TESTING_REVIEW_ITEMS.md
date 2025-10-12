# Testing Matrix - Human Review Items
**Date:** 2025-10-11
**Purpose:** Items requiring human judgment and strategic decision-making

This document contains testing decisions that require human review due to:
- Strategic trade-offs (time vs coverage)
- Design decisions (architecture impact)
- Resource constraints (CI/CD time, maintenance burden)
- Risk assessment (what's acceptable vs critical)

---

## 📋 Review Item 1: Integration Test Priorities

### Context
After analysis, the codebase has **strong unit test coverage** (~545 tests) but **gaps in integration tests** for cross-cutting scenarios.

### Decision Needed
**Which integration tests should be prioritized given limited time/resources?**

### Options

#### Option A: Critical Path First (Recommended)
**Focus:** Test what users actually do most often

**Priority 1: Installation workflow** (vm-installer)
- Risk: Installation failures block all usage
- Impact: High - affects all new users
- Estimated effort: 2-3 days
- Tests to add:
  - Full install from source
  - PATH updates across shells (bash, zsh, fish)
  - Upgrade scenarios
  - Permission handling

**Priority 2: Plugin system** (vm-plugin)
- Risk: Broken plugins = broken presets
- Impact: Medium - affects advanced users
- Estimated effort: 1 day
- Tests to add:
  - Plugin loading from directories
  - Invalid plugin handling
  - Plugin priority/override
  - Malformed YAML handling

**Priority 3: Docker registry integration** (vm-docker-registry)
- Risk: Registry failures = slow builds
- Impact: Medium - has fallback (Docker Hub)
- Estimated effort: 2 days
- Tests to add:
  - Pull-through caching
  - Docker daemon integration
  - Garbage collection
  - Network failure recovery

#### Option B: Coverage Completion
**Focus:** Fill all gaps systematically

- Same tests as Option A, plus:
- vm-platform cross-platform tests
- vm-core file system tests
- Multi-VM complex scenarios
- Service dependency chains

Estimated effort: 5-6 days

#### Option C: High-Value Scenarios Only
**Focus:** Test complex workflows, trust unit tests for basics

**Areas:**
- Multi-VM workflows (create multiple VMs, test interactions)
- Service dependency chains (postgres → redis → app)
- Error recovery (network failures, Docker crashes)
- Concurrent operations (multiple developers, shared services)

Estimated effort: 3-4 days

### My Recommendation: **Option A (Critical Path First)**

**Reasoning:**
1. Installation is the first impression - must work flawlessly
2. Unit tests cover most basic functionality already
3. Plugin system is core to user experience (presets)
4. Docker registry is "nice to have" optimization
5. Can iterate and add more tests later

**Trade-offs:**
- ✅ Pro: Focuses on user-facing issues
- ✅ Pro: Delivers value incrementally
- ⚠️  Con: Doesn't achieve "complete coverage"
- ⚠️  Con: Some edge cases remain untested

### Questions for Human
1. What's the user pain point ranking? (Installation vs plugins vs performance)
2. What's the CI/CD time budget? (More tests = longer CI)
3. Is 85% coverage acceptable, or is 95% required?
4. How much time can be allocated to this work?

---

## 📋 Review Item 2: Test Execution Time vs Coverage

### Context
Adding comprehensive integration tests will increase CI/CD time significantly.

**Current situation:**
- Unit tests: Fast (~30 seconds total)
- Integration tests: Moderate (~2-3 minutes with Docker)
- E2E tests: Slow (5-10 minutes per full workflow)

### Decision Needed
**What's the acceptable trade-off between test coverage and CI/CD speed?**

### Options

#### Option A: Fast CI, Comprehensive Nightly
**CI (on every push):**
- Unit tests only (~30s)
- Smoke tests (critical paths only, ~1-2 min)
- Total: <3 minutes

**Nightly (scheduled):**
- All integration tests
- All E2E tests
- Cross-platform matrix
- Total: ~30-45 minutes

**Trade-offs:**
- ✅ Pro: Fast feedback loop
- ✅ Pro: Doesn't block development
- ⚠️  Con: Integration issues found later
- ⚠️  Con: Possible main branch breakage

#### Option B: Moderate CI, No Nightly (Recommended)
**CI (on every push):**
- Unit tests (~30s)
- Integration tests (~5min)
- Critical E2E tests (~5min)
- Total: ~10 minutes

**No nightly needed**

**Trade-offs:**
- ✅ Pro: Catches issues early
- ✅ Pro: Simpler CI/CD setup
- ⚠️  Con: Slower feedback (10 vs 3 minutes)
- ✅ Pro: Main branch always healthy

#### Option C: Comprehensive CI
**CI (on every push):**
- Everything: unit, integration, E2E, cross-platform
- Total: ~20-30 minutes

**Trade-offs:**
- ✅ Pro: Maximum confidence
- ✅ Pro: No surprises
- ⚠️  Con: Slow feedback (developers wait)
- ⚠️  Con: High CI cost (if using paid runners)

### My Recommendation: **Option B (Moderate CI)**

**Reasoning:**
1. 10 minutes is acceptable for most teams
2. Catches issues before merge
3. Simpler than managing nightly builds
4. Most integration tests are fast anyway

**Implementation:**
```yaml
# .github/workflows/test.yml
on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1

      # Fast unit tests
      - name: Unit tests
        run: cargo test --workspace --lib
        timeout-minutes: 2

      # Integration tests
      - name: Integration tests
        run: cargo test --workspace --test '*'
        timeout-minutes: 8

      # Docker-dependent tests (if Docker available)
      - name: Docker tests
        if: runner.os == 'Linux'
        run: cargo test --workspace -- --ignored
        timeout-minutes: 5
```

### Questions for Human
1. What's the team's tolerance for CI wait time?
2. Are CI runners paid or free? (affects cost consideration)
3. How often do developers push? (affects productivity impact)
4. Is there a preference for fast feedback vs complete testing?

---

## 📋 Review Item 3: Test Organization Strategy

### Context
Current test organization mixes concerns:
- `vm/tests/` has 11 files with varying purposes
- Some test VM operations, some test CLI, some test services
- Unclear naming: "service_lifecycle" means different things in different files

### Decision Needed
**Should we reorganize tests for clarity, or keep current structure?**

### Current Structure
```
vm/tests/
├── workflow_tests.rs          # Config CLI
├── config_cli_tests.rs        # Config CLI (different aspects)
├── pkg_cli_tests.rs           # Package CLI
├── service_lifecycle_integration_tests.rs  # Service management
├── port_forwarding_tests.rs   # Port forwarding
├── temp_workflow_tests.rs     # Temp VMs
├── ssh_refresh.rs             # SSH features
├── vm_ops/
│   ├── create_destroy_tests.rs
│   ├── interaction_tests.rs
│   ├── service_lifecycle_tests.rs  # VM lifecycle!
│   ├── status_tests.rs
│   ├── feature_tests.rs
│   ├── lifecycle_integration_tests.rs
│   ├── multi_instance_tests.rs
│   └── provider_parity_tests.rs
```

### Options

#### Option A: Keep Current Structure
**Reasoning:** "Don't reorganize working code"

**Trade-offs:**
- ✅ Pro: Zero risk of breaking tests
- ✅ Pro: No immediate work needed
- ⚠️  Con: Confusing for new contributors
- ⚠️  Con: "service_lifecycle" ambiguity
- ⚠️  Con: No clear pattern for future tests

#### Option B: Reorganize by Feature (Recommended)
**Proposed structure:**
```
vm/tests/
├── cli/
│   ├── config_commands.rs     # Merge workflow_tests + config_cli_tests
│   ├── pkg_commands.rs        # pkg_cli_tests
│   └── vm_commands.rs         # High-level CLI
├── services/
│   ├── service_manager.rs     # Rename service_lifecycle_integration_tests
│   └── shared_services.rs     # Postgres, Redis, etc
├── vm_operations/
│   ├── lifecycle.rs           # create, destroy, start, stop
│   ├── interaction.rs         # ssh, exec, logs
│   ├── status.rs              # status, list
│   ├── features.rs            # Feature flags
│   └── multi_instance.rs      # Multi-instance
├── networking/
│   ├── port_forwarding.rs
│   └── ssh_refresh.rs
├── temp_vms/
│   └── temp_workflow.rs
└── integration/
    ├── provider_parity.rs
    └── full_lifecycle.rs
```

**Trade-offs:**
- ✅ Pro: Clear organization
- ✅ Pro: Easy to find tests
- ✅ Pro: Clear pattern for future tests
- ⚠️  Con: Requires refactoring work (~1-2 days)
- ⚠️  Con: Risk of breaking tests during move

#### Option C: Hybrid - Only Rename Confusing Files
**Rename only:**
- `service_lifecycle_integration_tests.rs` → `service_manager_integration_tests.rs`
- `vm_ops/service_lifecycle_tests.rs` → `vm_ops/vm_lifecycle_tests.rs`

**Keep everything else as-is**

**Trade-offs:**
- ✅ Pro: Minimal risk
- ✅ Pro: Fixes main confusion
- ⚠️  Con: Doesn't fully solve organization problem
- ✅ Pro: Low effort (~15 minutes)

### My Recommendation: **Option C (Hybrid)**

**Reasoning:**
1. The main issue is naming ambiguity ("service_lifecycle")
2. Two simple renames fix 90% of the confusion
3. Reorganizing isn't urgent (tests work fine)
4. Low risk, high value

**Implementation:**
```bash
# Rename to clarify scope
cd vm/tests
git mv service_lifecycle_integration_tests.rs service_manager_integration_tests.rs
git mv vm_ops/service_lifecycle_tests.rs vm_ops/vm_lifecycle_tests.rs

# Update internal references
sed -i 's/service_lifecycle_tests/vm_lifecycle_tests/g' vm_ops.rs
```

### Questions for Human
1. Is the current organization causing real problems?
2. Are new contributors getting confused?
3. Is there a style guide for test organization?
4. Worth spending 1-2 days on full reorganization?

---

## 📋 Review Item 4: Coverage Metrics and Goals

### Context
Current workspace has ~545 tests with good coverage, but no formal coverage targets.

### Decision Needed
**What are the coverage goals and how should they be measured?**

### Current State (Estimated)
Based on test counts and package sizes:
- vm-config: ~85% (excellent)
- vm-package-server: ~90% (excellent)
- vm: ~80% (good)
- vm-provider: ~75% (good)
- vm-auth-proxy: ~70% (good, but focused on critical paths)
- vm-installer: ~60% (unit tests only)
- vm-docker-registry: ~65% (unit tests only)
- vm-plugin: ~70% (unit tests only)
- vm-core: ~60% (gaps in file system, command stream)
- vm-platform: ~55% (gaps in cross-platform behavior)
- **Workspace average: ~72%**

### Options

#### Option A: Package-Specific Targets
**Different targets per package based on criticality:**

| Package | Target | Rationale |
|---------|--------|-----------|
| vm-auth-proxy | 90% | Security-critical |
| vm-package-server | 85% | Data integrity |
| vm-provider | 85% | Core functionality |
| vm-config | 80% | Complex logic |
| vm | 80% | User-facing |
| vm-installer | 80% | First impression |
| vm-docker-registry | 70% | Optional feature |
| vm-plugin | 70% | Simple logic |
| vm-core | 75% | Foundation |
| vm-platform | 65% | Platform-specific |
| Others | 60% | Utilities |

**Trade-offs:**
- ✅ Pro: Realistic and achievable
- ✅ Pro: Focuses effort where it matters
- ⚠️  Con: More complex to track
- ✅ Pro: Allows pragmatic decisions

#### Option B: Uniform 80% Target
**All packages must have 80% coverage**

**Trade-offs:**
- ✅ Pro: Simple to communicate
- ✅ Pro: Easy to measure
- ⚠️  Con: May be overkill for simple packages
- ⚠️  Con: May be insufficient for critical packages
- ⚠️  Con: "Gaming" possible (trivial tests to hit number)

#### Option C: Trend-Based (No Hard Targets) (Recommended)
**Focus on improving coverage over time, not hitting specific numbers**

**Rules:**
1. PRs cannot decrease coverage
2. New code must have tests
3. Bug fixes must include regression tests
4. No minimum percentage required

**Trade-offs:**
- ✅ Pro: Prevents "gaming" metrics
- ✅ Pro: Focus on quality, not quantity
- ✅ Pro: Flexible and pragmatic
- ⚠️  Con: Less concrete goal
- ⚠️  Con: Harder to track progress

### My Recommendation: **Option C (Trend-Based)**

**Reasoning:**
1. Coverage percentage can be misleading (100% coverage ≠ bug-free)
2. Quality of tests matters more than quantity
3. Current coverage (~72%) is already reasonable
4. Trend-based prevents coverage regression
5. Allows pragmatic decisions (skip trivial code)

**Implementation:**
```yaml
# .github/workflows/coverage.yml
- name: Check coverage trend
  run: |
    NEW_COV=$(cargo tarpaulin --workspace --out Xml | grep line-rate)
    BASE_COV=$(git show origin/main:.coverage | grep line-rate)
    if (( $(echo "$NEW_COV < $BASE_COV" | bc -l) )); then
      echo "Coverage decreased from $BASE_COV to $NEW_COV"
      exit 1
    fi
```

### Questions for Human
1. Is there an existing coverage policy?
2. What's more important: high coverage or quality tests?
3. Are there compliance requirements (e.g., 80% mandated)?
4. Should we track coverage at all, or focus on test quality?

---

## 📋 Review Item 5: Test Infrastructure Investment

### Context
Currently, each test file duplicates setup code (fixtures, helpers, assertions).

### Decision Needed
**Should we invest in shared test infrastructure (test-utils crate)?**

### Current Duplication Examples

**Fixture creation** (repeated in ~8 files):
```rust
struct WorkflowTestFixture {
    _temp_dir: TempDir,
    test_dir: PathBuf,
    binary_path: PathBuf,
}

impl WorkflowTestFixture {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let test_dir = temp_dir.path().join("test_project");
        fs::create_dir_all(&test_dir)?;
        // ... find binary path ...
        Ok(Self { _temp_dir, test_dir, binary_path })
    }
}
```

**Container cleanup** (repeated in ~5 files):
```rust
fn cleanup_test_containers(&self) -> Result<()> {
    let _ = Command::new("docker")
        .args(["rm", "-f", &self.project_name])
        .output();
    Ok(())
}
```

**Assertions** (custom patterns in multiple files):
```rust
assert!(
    output.status.success(),
    "Command failed: {}",
    String::from_utf8_lossy(&output.stderr)
);
```

### Options

#### Option A: Create test-utils Crate (Recommended)
**Create:** `rust/test-utils/`

**Contents:**
```rust
// test-utils/src/lib.rs
pub mod fixtures;     // Shared test fixtures
pub mod assertions;   // Custom assertions
pub mod docker;       // Docker test helpers
pub mod temp;         // Temp directory helpers
pub mod binary;       // Binary path resolution

// Examples:
pub use fixtures::VmTestFixture;
pub use docker::cleanup_containers;
pub use assertions::assert_success;
```

**Benefits:**
- ✅ Reduce code duplication (~500 LOC saved)
- ✅ Consistent test patterns
- ✅ Easier to write new tests
- ✅ Single place to fix bugs

**Costs:**
- ⚠️  Initial setup: ~1 day
- ⚠️  Migration: ~2 days
- ⚠️  Maintenance: minimal

**Trade-offs:**
- ✅ Pro: DRY principle
- ✅ Pro: Better test quality
- ⚠️  Con: Another dependency
- ⚠️  Con: Migration effort

#### Option B: Shared Helpers in tests/common/
**Create:** `vm/tests/common/mod.rs` with shared helpers

**Benefits:**
- ✅ Simpler than full crate
- ✅ Still reduces duplication
- ⚠️  Only available within vm package

**Costs:**
- ⚠️  Setup: ~4 hours
- ⚠️  Migration: ~1 day

#### Option C: Keep Current Duplication
**Do nothing**

**Trade-offs:**
- ✅ Pro: Zero effort
- ⚠️  Con: Continued duplication
- ⚠️  Con: Harder to maintain consistency
- ⚠️  Con: More code to review in PRs

### My Recommendation: **Option A (test-utils crate)**

**Reasoning:**
1. ~500 LOC duplication is significant
2. Multiple packages need same helpers
3. Investment pays off quickly (2-3 days work, saves hours on future tests)
4. Improves test quality through consistency
5. Standard pattern in Rust projects

**Phased Implementation:**
```markdown
Phase 1 (Day 1): Create test-utils crate
- Set up Cargo.toml
- Add basic fixtures module
- Add docker helpers module
- Add assertion module

Phase 2 (Day 2): Migrate vm tests
- Migrate vm/tests to use test-utils
- Verify all tests still pass

Phase 3 (Day 3): Migrate other packages
- Migrate vm-config tests
- Migrate vm-provider tests
- Migrate vm-package-server tests

Phase 4 (Optional): Advanced features
- Add contract test framework
- Add property-based testing helpers
- Add benchmark helpers
```

### Questions for Human
1. Is code duplication causing maintenance issues?
2. Is 3 days of work acceptable for this improvement?
3. Are there existing patterns/libraries preferred?
4. Should this be done now or deferred?

---

## 📋 Review Item 6: Cross-Platform Testing Strategy

### Context
The project supports Linux, macOS, and Windows, but most tests only run on Linux.

**Current situation:**
- Primary CI: Linux only
- tart_provider_tests: macOS only (correctly)
- No Windows-specific tests
- No cross-platform parity tests

### Decision Needed
**How much should we invest in cross-platform testing?**

### Options

#### Option A: Linux Primary, Others Secondary
**CI Matrix:**
- Linux: All tests (required to pass)
- macOS: All tests (required to pass)
- Windows: All tests (optional/allowed to fail)

**Rationale:** Most users on Linux/macOS, Windows is experimental

**Trade-offs:**
- ✅ Pro: Pragmatic (matches user base)
- ✅ Pro: Doesn't block development
- ⚠️  Con: Windows users may hit bugs
- ✅ Pro: Lower CI cost

#### Option B: Full Parity (Recommended)
**CI Matrix:**
- Linux: All tests (required)
- macOS: All tests (required)
- Windows: All tests (required)

**Rationale:** Professional quality, no platform is second-class

**Trade-offs:**
- ✅ Pro: Equal quality on all platforms
- ✅ Pro: Catches platform-specific bugs
- ⚠️  Con: Higher CI cost (~3x)
- ⚠️  Con: More flaky tests to handle
- ⚠️  Con: Slower feedback (wait for all platforms)

#### Option C: Staged Rollout
**Phase 1 (Now):**
- Linux: All tests (required)
- macOS: All tests (optional)
- Windows: Smoke tests only (optional)

**Phase 2 (After stabilization):**
- All platforms: All tests (required)

**Trade-offs:**
- ✅ Pro: Incremental approach
- ✅ Pro: Stabilize tests on Linux first
- ⚠️  Con: Temporary quality difference

### My Recommendation: **Option B (Full Parity)**

**Reasoning:**
1. Modern CI (GitHub Actions) makes this easy and free for OSS
2. Platform-specific bugs are hard to debug if found late
3. Sets high quality bar from start
4. Many Rust projects do this successfully

**Implementation:**
```yaml
# .github/workflows/test.yml
strategy:
  matrix:
    os: [ubuntu-latest, macos-latest, windows-latest]
    rust: [stable]
  fail-fast: false  # Don't cancel other jobs if one fails

runs-on: ${{ matrix.os }}
```

**Caveats:**
- Some tests may need platform-specific adjustments
- Docker availability varies by platform
- May need to mark some tests as platform-specific

### Questions for Human
1. What's the target user platform distribution?
2. Is Windows support a priority or nice-to-have?
3. Is there a CI cost constraint?
4. Acceptable to have slower CI for better quality?

---

## 📋 Summary & Decision Matrix

| Item | Recommendation | Confidence | Effort | Risk |
|------|---------------|------------|--------|------|
| 1. Test priorities | Option A: Critical path | 85% | 2-3 days | Low |
| 2. CI execution time | Option B: Moderate CI | 90% | 2 hours setup | Low |
| 3. Test organization | Option C: Hybrid rename | 95% | 15 minutes | Very low |
| 4. Coverage goals | Option C: Trend-based | 80% | 2 hours setup | Low |
| 5. Test infrastructure | Option A: test-utils crate | 85% | 3 days | Low |
| 6. Cross-platform | Option B: Full parity | 90% | 4 hours setup | Medium |

**Total Estimated Effort:** ~5-6 days for all recommendations

**Recommended Priority Order:**
1. **Item 3** (15 min) - Quick win, fixes confusion
2. **Item 2** (2 hours) - CI setup, enables other work
3. **Item 6** (4 hours) - Cross-platform CI, catches bugs early
4. **Item 5** (3 days) - Test infrastructure, improves developer experience
5. **Item 1** (2-3 days) - Integration tests, fills gaps
6. **Item 4** (2 hours) - Coverage tracking, optional but good practice

**Can be done in parallel:**
- Items 2, 3, 6 (CI/organization setup)
- Items 1, 5 (testing work)

---

## 🎯 Next Steps

1. **Human Review:** Review these recommendations and provide feedback
2. **Decision:** Choose options for each item (or accept recommendations)
3. **Prioritize:** Decide which items to tackle first
4. **Assign:** Allocate resources/time for implementation
5. **Execute:** Follow implementation plans in this document
6. **Track:** Monitor progress and adjust as needed

**Questions? Concerns? Alternative ideas?**
- Open for discussion on any recommendation
- Can provide more detail on any option
- Can analyze additional trade-offs if needed
