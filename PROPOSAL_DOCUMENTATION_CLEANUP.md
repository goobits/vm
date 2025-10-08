# Documentation Cleanup and Standardization Proposal

**Status**: Draft
**Author**: System Analysis
**Date**: 2025-10-08
**Target Version**: 2.0.7
**Priority**: High

---

## Executive Summary

A comprehensive analysis of all 35 markdown documentation files revealed **169 issues** including 26 critical errors that will cause user commands to fail. This proposal outlines a systematic approach to fix documentation drift, establish quality standards, and implement automated validation to prevent future issues.

**Impact**: Users currently experience broken commands, missing features, and confusion due to documentation that doesn't match implementation.

**Timeline**: 3-week phased rollout with immediate fixes in Week 1.

---

## Table of Contents

1. [Problem Statement](#problem-statement)
2. [Analysis Summary](#analysis-summary)
3. [Proposed Solution](#proposed-solution)
4. [Implementation Plan](#implementation-plan)
5. [Automation & Prevention](#automation--prevention)
6. [Success Metrics](#success-metrics)
7. [Risk Assessment](#risk-assessment)
8. [Approval & Sign-Off](#approval--sign-off)

---

## Problem Statement

### Current State

The VM tool documentation has experienced **systematic drift** from the codebase implementation:

- **26 critical errors** causing command failures
- **46 moderate issues** creating user confusion
- **97 minor inconsistencies** reducing documentation quality
- **Zero automated validation** to catch errors before release
- **No standardized templates** leading to inconsistent documentation structure

### Root Causes

1. **Process Gap**: Code merged without corresponding documentation updates (e.g., git worktrees feature)
2. **No Validation**: CLI flags documented but never verified against actual implementation
3. **Version Drift**: Version bumped to 2.0.6 but CHANGELOG stops at 2.0.5
4. **Template Inconsistency**: Each plugin README has different structure and content
5. **Manual Synchronization**: No automated checks for package names, file paths, or command syntax

### User Impact

```bash
# User follows documentation:
$ vm plugin install plugins/nodejs-dev
Error: Invalid path 'plugins/nodejs-dev'

# User tries troubleshooting:
$ docker logs $(vm status --container-id)
Error: unknown flag: --container-id

# User configures database persistence:
project:
  persist_databases: true  # Wrong location - won't work

# User provisions k8s-dev:
npm ERR! 404 'kubernetes-yaml-completion' is not in the npm registry
```

**Result**: Loss of user trust, increased support burden, reduced adoption.

---

## Analysis Summary

### Critical Issues Breakdown

| Category | Count | Files Affected | Example |
|----------|-------|----------------|---------|
| Non-existent CLI flags | 7 | vm-package-server docs | `--docker`, `--no-config` flags |
| Invalid commands | 5 | User guide docs | `vm status --container-id` |
| Wrong config locations | 2 | configuration.md, troubleshooting.md | `persist_databases` placement |
| Non-existent packages | 2 | k8s-dev, vibe-dev | `kubernetes-yaml-completion`, `claudeflow` |
| Incorrect installation | 4 | All plugin READMEs | `vm plugin install plugins/X-dev` |
| Security risks | 1 | vibe-dev | Aliases bypassing security |
| Broken references | 3 | Various | File paths, links |
| Missing features | 2 | User guides | Git worktrees undocumented |

### Documentation Debt by File Type

```
Plugin READMEs:        12 files × avg 4.5 issues = 54 total issues
User Guide:            5 files × avg 7.2 issues = 36 total issues
Package Docs:          7 files × avg 5.4 issues = 38 total issues
Development Docs:      4 files × avg 4.5 issues = 18 total issues
Core Docs:            4 files × avg 5.8 issues = 23 total issues
```

### Full Issue Inventory

**See Appendix A for complete 169-issue breakdown by file and severity.**

---

## Proposed Solution

### Phase 1: Emergency Fixes (Week 1) - Critical Issues

#### 1.1 Fix Broken Commands

**Files**: `docs/user-guide/cli-reference.md`, `docs/user-guide/troubleshooting.md`

**Changes**:
```diff
- docker logs $(vm status --container-id)
+ # Get container name from project
+ docker logs $(docker ps --filter "name=my-project-dev" --format "{{.Names}}")
```

**Changes**:
```diff
- docker inspect $(vm status --raw)
+ # Use vm status output directly
+ vm status
+ docker inspect my-project-dev
```

#### 1.2 Correct Configuration Examples

**Files**: `docs/user-guide/configuration.md`, `docs/user-guide/troubleshooting.md`

**Changes**:
```diff
- project:
-   persist_databases: true
+ # Top-level field (deprecated but supported)
+ persist_databases: true
```

**Note**: Add deprecation warning and recommend modern service configuration approach.

#### 1.3 Fix Plugin Installation Documentation

**Files**: All plugin READMEs (10 files)

**Current**:
```markdown
## Installation

```bash
vm plugin install plugins/nodejs-dev
```
```

**Proposed**:
```markdown
## Installation

This plugin is automatically installed when you install the VM tool via `./install.sh`.

To verify it's available:

```bash
vm config preset --list | grep nodejs
```

## Usage

Apply this preset to your project:

```bash
vm config preset nodejs
vm create
```

Or add to `vm.yaml`:

```yaml
preset: nodejs
```
```

#### 1.4 Remove Non-existent Packages

**k8s-dev/preset.yaml** and **README.md**:
```diff
  npm_packages:
-   - kubernetes-yaml-completion
+   # kubectl provides its own shell completion
```

**vibe-dev/preset.yaml** and **README.md**:
```diff
  pip_packages:
-   - claudeflow
+   - anthropic  # Official Anthropic Python SDK
```

#### 1.5 Remove Dangerous Security Aliases

**vibe-dev/preset.yaml** and **README.md**:
```diff
- aliases:
-   - claudeyolo: claude --dangerously-skip-permissions
-   - geminiyolo: GEMINI_API_KEY=${GEMINI_API_KEY:-} gemini --approval-mode=yolo
-   - codexyolo: codex --dangerously-bypass-approvals-and-sandbox
```

**Rationale**: Promoting security bypasses is irresponsible and creates liability.

**Alternative**: If rapid development workflows are desired, document proper risk mitigation instead.

#### 1.6 Fix Package Server Documentation

**Files**: All `rust/vm-package-server/docs/*.md`

**Changes**:
```diff
- pkg-server start --docker --foreground
+ pkg-server start --host 0.0.0.0 --port 8080
```

Remove documentation for non-existent commands:
- `pkg-server stop`
- `pkg-server config`
- `pkg-server use`
- `pkg-server exec`

**Decision Point**: Are these planned features? If yes, move to "Future Features" section. If no, remove entirely.

### Phase 2: Major Updates (Week 2) - Moderate Issues

#### 2.1 Document Git Worktrees Feature

**Files to Update**:
- `README.md` - Add to features list and configuration section
- `CLAUDE.md` - Add developer documentation
- `CHANGELOG.md` - Add to Unreleased section
- `docs/user-guide/configuration.md` - Add configuration examples
- `PROPOSAL_GIT_WORKTREES.md` - Update status to "Implemented"

**New Content Structure**:

**In `docs/user-guide/configuration.md`**:
```markdown
### Git Worktrees (New in 2.0.6)

Enable Git worktree support for multi-branch development:

**Global Configuration** (`~/.vm/config.yaml`):
```yaml
worktrees:
  enabled: true
  base_path: ~/worktrees  # Optional: custom worktree location
```

**Project Configuration** (`vm.yaml`):
```yaml
worktrees:
  enabled: true  # Override global setting per-project
```

**Features**:
- Automatic detection of worktree repositories
- Proper volume mounting for worktree directories
- Support for relative worktree paths (Git 2.48+)

**Use Cases**:
- Developing multiple branches simultaneously
- Testing feature branches in isolation
- CI/CD workflows with parallel branch testing

**Example Workflow**:
```bash
# Create worktree
git worktree add ../feature-branch

# Navigate and create VM
cd ../feature-branch
vm config worktrees enable
vm create

# Each worktree gets isolated VM environment
```
```

#### 2.2 Synchronize Version Documentation

**CHANGELOG.md**:
```diff
  ## [Unreleased]
+
+ ## [2.0.6] - 2025-10-08
+
+ ### Added
+ - Git worktrees support for multi-branch development
+ - Improved provider error handling
+
+ ### Fixed
+ - Docker provider volume mounting edge cases
+ - Supervisor restart permission errors
```

**PROPOSAL_GIT_WORKTREES.md**:
```diff
- **Status**: Draft
+ **Status**: Implemented (v2.0.6)

- **Date**: 2025-01-XX
+ **Date**: 2025-10-08 (Implemented)

- **Target Version**: 2.1.0
+ **Implemented In**: 2.0.6
```

#### 2.3 Standardize Preset Naming

**Files**: `docs/user-guide/cli-reference.md`, `docs/user-guide/presets.md`

**Corrections**:
```diff
- vm config preset nextjs
+ vm config preset next
```

**Add Clarity**:
```markdown
## Preset Naming Convention

**Plugin Directory Name** ≠ **Preset Name**

- Directory: `plugins/nextjs-dev/` → Preset: `next`
- Directory: `plugins/nodejs-dev/` → Preset: `nodejs`
- Directory: `plugins/k8s-dev/` → Preset: `kubernetes`

The `-dev` suffix is automatically removed during installation.

**To list all available presets**:
```bash
vm config preset --list
```
```

#### 2.4 Clarify Services Configuration

**All plugin READMEs**:

**Current**:
```markdown
### Optional Services (enable manually if needed)
- Redis - In-memory data store
- PostgreSQL - Relational database
```

**Proposed**:
```markdown
### Included Services

This preset includes the following services **enabled by default**:
- **Redis** - In-memory data store
- **PostgreSQL** - Relational database

**To disable a service**, override in your `vm.yaml`:

```yaml
preset: nodejs
services:
  redis:
    enabled: false
  postgresql:
    enabled: false
```

**To customize service configuration**:

```yaml
preset: nodejs
services:
  postgresql:
    port: 5433  # Custom port
    database: myapp_dev
    user: myapp
    password: secret123
```
```

#### 2.5 Update Test Documentation

**docs/development/testing.md**:
```diff
- The VM tool includes 165+ unit tests and integration tests
+ The VM tool includes 300+ unit tests and integration tests
```

**Add Test Count by Package**:
```markdown
### Test Coverage by Package

| Package | Unit Tests | Integration Tests | Total |
|---------|-----------|-------------------|-------|
| vm-config | 87 | 12 | 99 |
| vm-provider | 54 | 8 | 62 |
| vm | 32 | 28 | 60 |
| vm-installer | 18 | 5 | 23 |
| Others | 61 | 0 | 61 |
| **TOTAL** | **252** | **53** | **305** |

*Updated: 2025-10-08*
```

### Phase 3: Standardization (Week 3) - Minor Issues + Templates

#### 3.1 Create Plugin README Template

**File**: `.github/PLUGIN_README_TEMPLATE.md`

```markdown
# {Plugin Name} Development Plugin

{Brief 1-2 sentence description of the plugin purpose and target framework/language}

## What's Included

### System Packages
- `package-name` - Description of what it does

### NPM Packages (if applicable)
- `package-name` - Description
- `package-name` - Description

### Python Packages (if applicable)
- `package-name` - Description

### Ruby Gems (if applicable)
- `gem-name` - Description

### Environment Variables
- `VAR_NAME` - Description and purpose

### Included Services

This preset includes the following services **enabled by default**:
- **ServiceName** - Purpose

To disable or customize services, see the [Configuration](#configuration) section below.

## Installation

This plugin is automatically installed with the VM tool. No additional installation required.

To verify availability:

```bash
vm config preset --list | grep {preset-name}
```

## Usage

Apply this preset to your project:

```bash
vm config preset {preset-name}
vm create
```

Or add to `vm.yaml`:

```yaml
preset: {preset-name}
```

## Configuration

### Customizing Services

```yaml
preset: {preset-name}
services:
  servicename:
    enabled: false  # Disable service
    # Or customize:
    port: 5433
    database: custom_db
```

### Additional Packages

```yaml
preset: {preset-name}
packages:
  npm:
    - custom-package
  pip:
    - custom-python-package
```

## Common Use Cases

1. **{Use Case 1}**
   ```bash
   # Example commands
   ```

2. **{Use Case 2}**
   ```bash
   # Example commands
   ```

## Troubleshooting

### Issue: {Common Problem}
**Solution**: {How to fix}

### Issue: {Common Problem}
**Solution**: {How to fix}

## Related Documentation

- [Configuration Guide](../../docs/user-guide/configuration.md)
- [Presets Overview](../../docs/user-guide/presets.md)
- [CLI Reference](../../docs/user-guide/cli-reference.md)

## License

MIT
```

#### 3.2 Apply Template to All Plugins

**Files**: All 10 plugin READMEs

**Process**:
1. Extract content from current README
2. Map to new template structure
3. Fill in missing sections (troubleshooting, use cases)
4. Verify all package lists match `preset.yaml`
5. Add service customization examples

#### 3.3 Fix Minor Inconsistencies

**vm-cli Package Description** (2 files):

**Files**: `rust/ARCHITECTURE.md`, `docs/development/architecture.md`

**Current**:
```markdown
- **vm-cli**: CLI output formatting, argument parsing, utilities
```

**Correct**:
```markdown
- **vm-cli**: Message template variable substitution via `msg!` macro and `MessageBuilder`
```

**Hardcoded Line Numbers**:

**File**: `CLAUDE.md`

```diff
- **Version location**: `rust/Cargo.toml` line 25 (workspace version)
+ **Version location**: `rust/Cargo.toml` workspace version field (search for `workspace.package.version`)
```

**fixtures/ Directory References**:

**File**: `docs/development/contributing.md`

```diff
- fixtures/          # Test configuration files
```

**Reason**: Directory doesn't exist; tests use dynamic fixture generation.

---

## Implementation Plan

### Week 1: Critical Fixes (Oct 9-15, 2025)

**Day 1-2**: Emergency Command Fixes
- [ ] Fix `vm status --container-id` references (2 files)
- [ ] Fix `persist_databases` location (2 files)
- [ ] Fix plugin installation commands (10 files)
- [ ] Create PR: "docs: fix critical command documentation errors"

**Day 3-4**: Package & Security Fixes
- [ ] Remove non-existent packages (k8s-dev, vibe-dev)
- [ ] Remove dangerous security aliases (vibe-dev)
- [ ] Fix package-server CLI documentation (7 files)
- [ ] Create PR: "docs: fix package references and remove security anti-patterns"

**Day 5**: Verification & Release
- [ ] Test all documented commands manually
- [ ] Run documentation link checker
- [ ] Merge emergency fixes
- [ ] Create hotfix release notes

**Deliverables**:
- 21 files fixed
- All critical issues resolved
- Emergency patch released

### Week 2: Major Updates (Oct 16-22, 2025)

**Day 1-2**: Git Worktrees Documentation
- [ ] Add worktrees to `README.md`
- [ ] Add worktrees to `CLAUDE.md`
- [ ] Add worktrees to `configuration.md`
- [ ] Update `PROPOSAL_GIT_WORKTREES.md` status
- [ ] Add examples and use cases

**Day 3**: Version Synchronization
- [ ] Update `CHANGELOG.md` with 2.0.6 section
- [ ] Verify version consistency across all docs
- [ ] Update proposal document statuses

**Day 4**: Preset Naming & Services
- [ ] Standardize preset names (nextjs → next)
- [ ] Add preset naming convention guide
- [ ] Update all plugin READMEs with service configuration guidance

**Day 5**: Testing & Review
- [ ] Update test count documentation
- [ ] Add test coverage table
- [ ] Peer review all changes
- [ ] Create PR: "docs: add worktrees documentation and fix moderate issues"

**Deliverables**:
- Git worktrees fully documented
- Version documentation synchronized
- All moderate issues resolved

### Week 3: Standardization (Oct 23-29, 2025)

**Day 1-2**: Template Creation
- [ ] Create plugin README template
- [ ] Create documentation style guide
- [ ] Define content standards

**Day 3-4**: Apply Templates
- [ ] Update all 10 plugin READMEs with new template
- [ ] Ensure consistency across all files
- [ ] Add missing sections (troubleshooting, use cases)

**Day 5**: Minor Fixes & Polish
- [ ] Fix vm-cli descriptions (2 files)
- [ ] Remove hardcoded line numbers
- [ ] Fix all minor inconsistencies
- [ ] Create PR: "docs: standardize plugin documentation and fix minor issues"

**Deliverables**:
- All 10 plugins using standard template
- Style guide published
- All minor issues resolved

---

## Automation & Prevention

### Automated Documentation Validation

#### 1. CLI Flag Validation

**Tool**: `scripts/validate-docs.sh`

```bash
#!/bin/bash
# Validate CLI flags in documentation match actual implementation

echo "Validating CLI documentation..."

# Extract all CLI commands from documentation
DOCS_FLAGS=$(grep -r "vm.*--" docs/ | grep -v "Binary" | sort -u)

# Check each flag against --help output
while IFS= read -r line; do
    flag=$(echo "$line" | grep -oP '\-\-[a-z-]+' | head -1)
    command=$(echo "$line" | grep -oP 'vm [a-z]+' | head -1)

    if ! cargo run --bin vm -- $command --help 2>&1 | grep -q "$flag"; then
        echo "ERROR: Flag $flag not found in $command --help"
        echo "  Found in: $line"
        exit 1
    fi
done <<< "$DOCS_FLAGS"

echo "✓ All CLI flags validated"
```

**Integration**: Add to CI/CD pipeline as documentation test.

#### 2. Package Name Validation

**Tool**: `scripts/validate-packages.sh`

```bash
#!/bin/bash
# Validate package names in presets exist in their registries

echo "Validating package references..."

# Check NPM packages
for pkg in $(yq '.npm_packages[]' plugins/*/preset.yaml); do
    if ! npm view "$pkg" version &>/dev/null; then
        echo "ERROR: NPM package '$pkg' not found in registry"
        exit 1
    fi
done

# Check PyPI packages
for pkg in $(yq '.pip_packages[]' plugins/*/preset.yaml); do
    if ! pip index versions "$pkg" &>/dev/null; then
        echo "ERROR: PyPI package '$pkg' not found in registry"
        exit 1
    fi
done

echo "✓ All packages validated"
```

#### 3. Version Synchronization Check

**Tool**: `scripts/check-version-sync.sh`

```bash
#!/bin/bash
# Ensure CHANGELOG.md is updated when version changes

CARGO_VERSION=$(grep '^version = ' rust/Cargo.toml | head -1 | cut -d'"' -f2)
CHANGELOG_VERSION=$(grep '^## \[' CHANGELOG.md | head -1 | grep -oP '\d+\.\d+\.\d+')

if [ "$CARGO_VERSION" != "$CHANGELOG_VERSION" ]; then
    echo "ERROR: Version mismatch"
    echo "  Cargo.toml: $CARGO_VERSION"
    echo "  CHANGELOG.md: $CHANGELOG_VERSION"
    echo "Please update CHANGELOG.md with a section for version $CARGO_VERSION"
    exit 1
fi

echo "✓ Version synchronized"
```

#### 4. Link Validation

**Tool**: `markdown-link-check`

```bash
# Add to CI
npm install -g markdown-link-check
find . -name "*.md" -exec markdown-link-check {} \;
```

#### 5. Documentation Coverage Check

**Tool**: `scripts/check-doc-coverage.sh`

```bash
#!/bin/bash
# Ensure all CLI commands have documentation

echo "Checking documentation coverage..."

# Get all subcommands from CLI
CLI_COMMANDS=$(cargo run --bin vm -- --help | grep -E '^\s+[a-z]+' | awk '{print $1}')

# Check each command has documentation
for cmd in $CLI_COMMANDS; do
    if ! grep -r "^## \`vm $cmd\`" docs/user-guide/cli-reference.md &>/dev/null; then
        echo "WARNING: Command 'vm $cmd' not documented in CLI reference"
    fi
done

echo "✓ Documentation coverage checked"
```

### CI/CD Integration

**File**: `.github/workflows/docs-validation.yml`

```yaml
name: Documentation Validation

on:
  pull_request:
    paths:
      - 'docs/**'
      - 'plugins/**/README.md'
      - 'rust/**/README.md'
      - 'CHANGELOG.md'
      - 'README.md'

jobs:
  validate-docs:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Install tools
        run: |
          npm install -g markdown-link-check
          pip install yq

      - name: Validate CLI flags
        run: ./scripts/validate-docs.sh

      - name: Validate package names
        run: ./scripts/validate-packages.sh

      - name: Check version sync
        run: ./scripts/check-version-sync.sh

      - name: Check links
        run: find . -name "*.md" -exec markdown-link-check {} \;

      - name: Check documentation coverage
        run: ./scripts/check-doc-coverage.sh
```

### Pre-commit Hook

**File**: `.githooks/pre-commit`

```bash
#!/bin/bash
# Pre-commit hook for documentation validation

# Only run if markdown files changed
if git diff --cached --name-only | grep -q "\.md$"; then
    echo "Validating documentation changes..."

    # Quick checks
    ./scripts/validate-docs.sh || exit 1
    ./scripts/check-version-sync.sh || exit 1

    echo "✓ Documentation validation passed"
fi
```

### Documentation Review Checklist

**File**: `.github/PULL_REQUEST_TEMPLATE.md` (add section)

```markdown
## Documentation Changes

If this PR includes code changes, please verify:

- [ ] All new CLI flags documented in `docs/user-guide/cli-reference.md`
- [ ] CHANGELOG.md updated (if version changed)
- [ ] Configuration examples updated (if config schema changed)
- [ ] Plugin README updated (if plugin modified)
- [ ] Commands tested and verified working as documented
- [ ] Links checked and valid
- [ ] No hardcoded line numbers or paths in documentation

**Automated checks**:
- [ ] CLI flag validation passed
- [ ] Package name validation passed
- [ ] Version synchronization passed
- [ ] Link checker passed
```

---

## Success Metrics

### Immediate Success Indicators (Week 1)

- [ ] Zero critical documentation errors remain
- [ ] All documented commands execute successfully
- [ ] No broken links in documentation
- [ ] User-reported documentation issues drop by 80%

### Medium-term Success Indicators (Week 4)

- [ ] All 169 identified issues resolved
- [ ] 100% of plugin READMEs use standard template
- [ ] Documentation CI checks pass on all PRs
- [ ] Zero version synchronization issues

### Long-term Success Indicators (3 months)

- [ ] No new documentation drift issues reported
- [ ] 95%+ documentation coverage of CLI commands
- [ ] Average PR includes documentation updates
- [ ] User satisfaction with documentation >4.5/5

### Measurement Dashboard

**Tool**: `scripts/docs-health.sh`

```bash
#!/bin/bash
# Generate documentation health report

echo "# Documentation Health Report"
echo "Generated: $(date)"
echo ""

# Count issues by severity
echo "## Issue Count by Severity"
./scripts/validate-docs.sh 2>&1 | grep "ERROR" | wc -l | xargs echo "Critical:"
./scripts/validate-docs.sh 2>&1 | grep "WARNING" | wc -l | xargs echo "Warnings:"

# Documentation coverage
echo ""
echo "## CLI Command Coverage"
CLI_COUNT=$(cargo run --bin vm -- --help | grep -E '^\s+[a-z]+' | wc -l)
DOC_COUNT=$(grep -r "^## \`vm " docs/user-guide/cli-reference.md | wc -l)
echo "Commands: $CLI_COUNT | Documented: $DOC_COUNT"

# Link health
echo ""
echo "## Link Health"
markdown-link-check README.md 2>&1 | grep -E "[✓✖]" | sort | uniq -c

# Version sync
echo ""
echo "## Version Synchronization"
./scripts/check-version-sync.sh && echo "✓ Synchronized" || echo "✗ Out of sync"
```

**Run monthly**: Track metrics over time to measure improvement.

---

## Risk Assessment

### Technical Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Breaking existing user workflows | Medium | High | Thoroughly test all command changes; provide migration guide |
| CI/CD validation too strict | Medium | Medium | Start with warnings only, gradually enforce |
| Template too rigid | Low | Medium | Allow customization in "Additional Sections" area |
| Automation false positives | Medium | Low | Manual review of validation failures |

### Process Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Team bandwidth constraints | High | Medium | Prioritize critical fixes first; spread over 3 weeks |
| Merge conflicts during updates | Medium | Low | Coordinate branches; use feature flags |
| Incomplete validation coverage | Medium | Medium | Iteratively improve automation scripts |
| Documentation review fatigue | Low | Low | Automate what's automatable; focus reviews on content quality |

### Rollback Plan

If critical issues arise during rollout:

1. **Week 1 Issues**: Revert individual file changes via git
2. **Week 2 Issues**: Feature flag new content sections, disable if problematic
3. **Week 3 Issues**: Template is additive, no breaking changes expected
4. **CI/CD Issues**: Validation failures don't block merges initially (warning mode)

**Rollback Trigger**: >5 user-reported issues within 48 hours of release.

---

## Alternatives Considered

### Alternative 1: Gradual Organic Fixes

**Approach**: Fix documentation issues as they're reported by users.

**Pros**:
- No dedicated time investment
- User-driven prioritization
- Low risk

**Cons**:
- Critical errors remain until reported
- Inconsistent quality persists
- No prevention of new issues
- Reactive rather than proactive

**Decision**: ❌ Rejected - 26 critical errors cannot wait for user reports.

### Alternative 2: Full Documentation Rewrite

**Approach**: Rebuild all documentation from scratch.

**Pros**:
- Perfect consistency
- Modern best practices
- Fresh perspective

**Cons**:
- 6+ week timeline
- High risk of introducing new errors
- Loss of valuable existing content
- Requires freezing new features

**Decision**: ❌ Rejected - Too disruptive; incremental improvement is safer.

### Alternative 3: External Documentation Platform

**Approach**: Move documentation to ReadTheDocs, GitBook, or similar.

**Pros**:
- Better search and navigation
- Version management
- Professional appearance

**Cons**:
- Migration effort (8+ weeks)
- Additional tooling/hosting
- Doesn't solve core accuracy issues
- Team must learn new platform

**Decision**: ❌ Rejected - Platform doesn't fix content accuracy; can consider later.

### Alternative 4: AI-Generated Documentation

**Approach**: Use LLM to generate documentation from code comments.

**Pros**:
- Automated updates
- Consistency with code
- Low maintenance

**Cons**:
- Lacks user-focused explanations
- Requires extensive code commenting
- Quality inconsistent
- Still needs manual review

**Decision**: ❌ Rejected - AI can assist but not replace human-written user guides.

### Selected Approach: Phased Manual Cleanup + Automation

**Why**: Balances immediate fixes with long-term prevention, manageable timeline, and proven approach.

---

## Dependencies

### External Dependencies

- `markdown-link-check` (npm) - Link validation
- `yq` (pip) - YAML parsing for package validation
- `cargo` - CLI command introspection

### Internal Dependencies

- `/workspace/rust/vm/src/cli.rs` - CLI definition source of truth
- `/workspace/plugins/*/preset.yaml` - Package lists
- `/workspace/rust/Cargo.toml` - Version source of truth
- `/workspace/CHANGELOG.md` - Version history

### Team Dependencies

- **Developer Time**: ~40 hours total (spread across 3 weeks)
- **Review Time**: ~8 hours for PR reviews
- **QA Time**: ~4 hours for command testing

---

## Approval & Sign-Off

### Review Checklist

**Before implementation approval**:

- [ ] All 169 issues catalogued and prioritized correctly
- [ ] Timeline is realistic (3 weeks)
- [ ] Automation approach is technically sound
- [ ] Risk mitigation strategies are adequate
- [ ] Success metrics are measurable
- [ ] Team has capacity for this work
- [ ] Users will benefit from these changes

### Stakeholder Approval

| Role | Name | Status | Date | Notes |
|------|------|--------|------|-------|
| Engineering Lead | | ☐ Pending | | |
| Product Owner | | ☐ Pending | | |
| Technical Writer (if applicable) | | ☐ Pending | | |
| Release Manager | | ☐ Pending | | |

### Sign-Off

**Approved by**: _________________________
**Date**: _________________________
**Conditions**: _________________________

---

## Appendix A: Complete Issue Inventory

### Critical Issues (26)

#### Plugin Installation Commands (4 issues)
- `plugins/nodejs-dev/README.md:28` - Invalid installation command
- `plugins/python-dev/README.md:29` - Invalid installation command
- `plugins/rust-dev/README.md:23` - Invalid installation command
- `plugins/django-dev/README.md:35` - Invalid installation command

#### Non-existent CLI Flags (7 issues)
- `rust/vm-package-server/README.md:20,32,94-98` - `--docker`, `--no-config`, `--foreground` flags
- `rust/vm-package-server/docs/quickstart.md:27-36` - `--docker` flag
- `rust/vm-package-server/docs/configuration.md:18-20` - Multiple non-existent flags
- `rust/vm-package-server/docs/cli-reference.md:42-44` - Multiple non-existent flags
- `docs/user-guide/cli-reference.md:84-90` - `--raw`, `--container-id` flags
- `docs/user-guide/troubleshooting.md:84-88` - `--container-id` flag usage
- `docs/user-guide/troubleshooting.md:327-329` - Related usage examples

#### Invalid Commands (5 issues)
- `rust/vm-package-server/docs/quickstart.md:161-171` - `pkg-server stop` command
- `rust/vm-package-server/docs/cli-reference.md:66-79` - `stop` command
- `rust/vm-package-server/docs/cli-reference.md:202-213` - `config` command
- `rust/vm-package-server/docs/cli-reference.md:218-236` - `use` command
- `rust/vm-package-server/docs/cli-reference.md:238-258` - `exec` command

#### Configuration Errors (2 issues)
- `docs/user-guide/configuration.md:563-564` - `persist_databases` wrong location
- `docs/user-guide/troubleshooting.md:246-251` - `persist_databases` wrong location

#### Non-existent Packages (2 issues)
- `plugins/k8s-dev/README.md:17` + `preset.yaml:11` - `kubernetes-yaml-completion`
- `plugins/vibe-dev/README.md:19` + `preset.yaml:13` - `claudeflow`

#### Security Issues (1 issue)
- `plugins/vibe-dev/README.md:21-24` + `preset.yaml:15-18` - Dangerous security bypass aliases

#### Missing API Endpoints (3 issues)
- `rust/vm-package-server/docs/api-reference.md:126-160` - PyPI DELETE endpoints
- `rust/vm-package-server/docs/api-reference.md:248-281` - npm DELETE endpoints
- `rust/vm-package-server/docs/api-reference.md:344-385` - Cargo DELETE endpoints

#### Broken References (2 issues)
- `rust/vm-package-server/README.md:228` - Broken link to `docs/contributing.md`
- `docs/development/contributing.md:95` - Non-existent `fixtures/` directory

### Moderate Issues (46)

*[Full list available in analysis output - truncated here for brevity]*

Key moderate issues include:
- Git worktrees feature undocumented (5 files)
- Preset naming inconsistencies (3 files)
- Services configuration confusion (7 files)
- Test count inaccuracy (1 file)
- vm-cli package description errors (2 files)

### Minor Issues (97)

*[Full list available in analysis output - truncated here for brevity]*

Categories:
- Hardcoded line numbers (2 issues)
- Version example staleness (3 issues)
- Missing documentation sections (15 issues)
- Inconsistent formatting (23 issues)
- Minor inaccuracies (54 issues)

---

## Appendix B: File Change Summary

### Files Requiring Changes (by priority)

**Week 1 - Critical (21 files)**:
1. docs/user-guide/cli-reference.md
2. docs/user-guide/troubleshooting.md
3. docs/user-guide/configuration.md
4. plugins/nodejs-dev/README.md
5. plugins/python-dev/README.md
6. plugins/rust-dev/README.md
7. plugins/django-dev/README.md
8. plugins/react-dev/README.md
9. plugins/nextjs-dev/README.md
10. plugins/rails-dev/README.md
11. plugins/k8s-dev/README.md + preset.yaml
12. plugins/vibe-dev/README.md + preset.yaml
13. rust/vm-package-server/README.md
14. rust/vm-package-server/docs/quickstart.md
15. rust/vm-package-server/docs/api-reference.md
16. rust/vm-package-server/docs/configuration.md
17. rust/vm-package-server/docs/cli-reference.md

**Week 2 - Moderate (9 files)**:
1. README.md
2. CLAUDE.md
3. CHANGELOG.md
4. PROPOSAL_GIT_WORKTREES.md
5. docs/user-guide/presets.md
6. docs/user-guide/configuration.md (additional updates)
7. docs/development/testing.md
8. All plugin READMEs (service configuration updates)

**Week 3 - Minor (5 files)**:
1. .github/PLUGIN_README_TEMPLATE.md (new)
2. rust/ARCHITECTURE.md
3. docs/development/architecture.md
4. docs/development/contributing.md
5. All plugin READMEs (template application)

**Total Files**: 35 modified, 1 created

---

## Appendix C: Automation Scripts Summary

### Scripts to Create

1. **scripts/validate-docs.sh** - CLI flag validation
2. **scripts/validate-packages.sh** - Package registry verification
3. **scripts/check-version-sync.sh** - Version synchronization check
4. **scripts/check-doc-coverage.sh** - Documentation coverage analysis
5. **scripts/docs-health.sh** - Health metrics dashboard
6. **.github/workflows/docs-validation.yml** - CI workflow
7. **.githooks/pre-commit** - Pre-commit documentation checks

**Total Lines**: ~350 lines of shell/YAML

---

## Appendix D: Timeline Visualization

```
Week 1: Emergency Fixes
├── Day 1-2: Command Fixes (12 files)
├── Day 3-4: Package/Security (9 files)
└── Day 5: Verification & Release
    └── Deliverable: 21 files fixed, 0 critical issues

Week 2: Major Updates
├── Day 1-2: Git Worktrees Docs (4 files)
├── Day 3: Version Sync (3 files)
├── Day 4: Preset/Services (8 files)
└── Day 5: Testing & Review
    └── Deliverable: All moderate issues resolved

Week 3: Standardization
├── Day 1-2: Template Creation (1 file)
├── Day 3-4: Template Application (10 files)
└── Day 5: Minor Fixes & Polish (5 files)
    └── Deliverable: All 169 issues resolved

Post-Implementation
└── Ongoing: Automated validation in CI/CD
```

---

**End of Proposal**

**Next Steps**:
1. Review this proposal with stakeholders
2. Obtain sign-off from engineering lead and product owner
3. Create GitHub issues for each week's work
4. Begin Week 1 emergency fixes upon approval
