# Proposal: CI/CD Automated Onboarding Test

**Status:** 🧪 Onboarding Test - Path 3
**Priority:** P0
**Complexity:** Medium (Automation Testing)
**Estimated Effort:** 20-40 minutes
**Test Type:** Automated Setup - Zero Human Interaction

---

## Purpose

This proposal tests the **fully automated onboarding experience** for:
- 🤖 CI/CD pipelines (GitHub Actions, GitLab CI, Jenkins)
- 🐳 Container-based builds
- 📦 Automated deployment workflows
- 🔄 Scripted environment provisioning
- 🎯 **Goal: Zero-touch installation and usage**

**Goal:** Validate that the VM tool can be installed and used in non-interactive, automated environments without human intervention.

---

## Test Persona

**Name:** CI/CD Pipeline (GitHub Actions Runner)
**Background:**
- Automated environment
- No TTY (terminal) available
- Cannot answer prompts
- Needs deterministic, repeatable setup
- Must complete without errors

**Pain Points to Test:**
- Can it install without prompts?
- Does it work in non-interactive mode?
- Are there TTY/color assumptions?
- Can it be scripted end-to-end?
- Does it integrate with CI/CD tools?

---

## Test Environment

**Supported CI Platforms:**
- ✅ GitHub Actions (Ubuntu runner)
- ✅ GitLab CI (Docker executor)
- ✅ Jenkins (Docker agent)
- ✅ Generic Docker container

**Starting State:**
```bash
# Fresh container/runner
which cargo      # May or may not exist
which docker     # Usually exists in CI
which vm         # Should not exist
TERM=dumb        # Non-interactive terminal
CI=true          # Standard CI environment variable
```

---

## Test Procedure

### Test 1: GitHub Actions Workflow

**Objective:** Can we use VM tool in GitHub Actions?

Create `.github/workflows/test-vm-onboarding.yml`:

```yaml
name: VM Tool Onboarding Test

on:
  workflow_dispatch:
  push:
    branches: [test-vm-onboarding]

jobs:
  test-vm-onboarding:
    runs-on: ubuntu-latest

    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Set up Rust toolchain
        uses: dtolnay/rust-toolchain@stable

      - name: Install VM tool
        run: |
          set -e
          echo "Installing VM tool..."
          cargo install vm
          vm --version
        timeout-minutes: 10

      - name: Create test project
        run: |
          mkdir -p test-project
          cd test-project
          echo '{"name": "ci-test", "version": "1.0.0"}' > package.json
          echo "console.log('Hello from CI');" > index.js

      - name: Create VM (non-interactive)
        run: |
          cd test-project
          vm create --force --non-interactive
        timeout-minutes: 5
        env:
          VM_NO_PROMPT: "true"

      - name: Verify VM status
        run: |
          vm list
          vm status

      - name: Execute command in VM
        run: |
          vm exec "node --version"
          vm exec "npm --version"
          vm exec "cat package.json"

      - name: Test SSH (non-interactive)
        run: |
          vm ssh --command "echo 'SSH works in CI'"
          vm ssh --command "ls -la"

      - name: Cleanup
        if: always()
        run: |
          vm destroy --force --all
          docker ps -a  # Verify cleanup
```

**Record:**
- [ ] Workflow file created: (Yes/No): _____
- [ ] Workflow runs without errors: (Yes/No): _____
- [ ] Installation time: _____ seconds
- [ ] VM creation time: _____ seconds
- [ ] Total workflow time: _____ seconds
- [ ] All steps pass: (Yes/No): _____
- [ ] Errors encountered: _____

### Test 2: Docker-Based CI (GitLab/Jenkins Style)

**Objective:** Can we use VM tool in Docker-based CI?

Create `test-ci-docker.sh`:

```bash
#!/bin/bash
set -euo pipefail

# Simulate CI environment
export CI=true
export TERM=dumb
export DEBIAN_FRONTEND=noninteractive

echo "=== CI/CD Automated Onboarding Test ==="

# Step 1: Install Rust (if needed)
if ! command -v cargo &> /dev/null; then
    echo "Installing Rust toolchain..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    source "$HOME/.cargo/env"
fi

# Step 2: Install VM tool
echo "Installing VM tool..."
time cargo install vm

# Verify installation
vm --version || { echo "ERROR: VM not installed"; exit 1; }

# Step 3: Create test project
echo "Creating test project..."
TEST_DIR=$(mktemp -d)
cd "$TEST_DIR"

cat > package.json <<'EOF'
{
  "name": "ci-test-app",
  "version": "1.0.0",
  "scripts": {
    "test": "echo 'Tests pass'"
  }
}
EOF

cat > index.js <<'EOF'
console.log('Hello from automated CI');
EOF

# Step 4: Create VM (non-interactive)
echo "Creating VM..."
VM_NO_PROMPT=true vm create --force

# Step 5: Run tests
echo "Testing VM commands..."

# Test list
vm list

# Test status
vm status

# Test exec
vm exec "node --version"
vm exec "npm --version"
vm exec "cat package.json"
vm exec "npm test"

# Test SSH with command
vm ssh --command "echo 'CI can SSH'"
vm ssh --command "ls -la"
vm ssh --command "pwd"

# Step 6: Cleanup
echo "Cleaning up..."
vm destroy --force --all

# Verify cleanup
if docker ps -a --format '{{.Names}}' | grep -q 'ci-test'; then
    echo "ERROR: Cleanup failed, containers still exist"
    exit 1
fi

echo "=== All CI/CD tests passed ==="
```

**Run the test:**
```bash
chmod +x test-ci-docker.sh
./test-ci-docker.sh
```

**Record:**
- [ ] Script runs to completion: (Yes/No): _____
- [ ] Exit code 0: (Yes/No): _____
- [ ] No prompts required: (Yes/No): _____
- [ ] All commands succeed: (Yes/No): _____
- [ ] Cleanup successful: (Yes/No): _____
- [ ] Total execution time: _____ seconds
- [ ] Errors: _____

### Test 3: Dockerfile Integration

**Objective:** Can VM tool be used in multi-stage Docker builds?

Create `Dockerfile.ci`:

```dockerfile
# Stage 1: Build environment with VM tool
FROM rust:1.70 AS builder

# Install VM tool
RUN cargo install vm

# Verify installation
RUN vm --version

# Stage 2: Runtime environment
FROM ubuntu:22.04

# Copy VM binary from builder
COPY --from=builder /usr/local/cargo/bin/vm /usr/local/bin/vm

# Install Docker CLI (to manage nested containers)
RUN apt-get update && \
    apt-get install -y docker.io && \
    rm -rf /var/lib/apt/lists/*

# Create test project
WORKDIR /app
RUN echo '{"name": "docker-ci-test", "version": "1.0.0"}' > package.json

# Test that VM can be invoked
RUN vm --version
RUN vm --help

# Entry point for CI tests
CMD ["vm", "create", "--force"]
```

**Build and test:**
```bash
docker build -f Dockerfile.ci -t vm-ci-test .
docker run --rm -v /var/run/docker.sock:/var/run/docker.sock vm-ci-test
```

**Record:**
- [ ] Docker build succeeds: (Yes/No): _____
- [ ] VM binary works in container: (Yes/No): _____
- [ ] Can create VMs from container: (Yes/No): _____
- [ ] Build time: _____ seconds
- [ ] Issues: _____

### Test 4: Non-Interactive Flag Testing

**Objective:** Test all commands in non-interactive mode

Create `test-non-interactive.sh`:

```bash
#!/bin/bash
set -euo pipefail

export CI=true
export VM_NO_PROMPT=true

# Ensure VM is installed
vm --version

# Create test project
TEST_DIR=$(mktemp -d)
cd "$TEST_DIR"
echo '{"name": "test"}' > package.json

echo "Testing non-interactive commands..."

# Test 1: Create without prompts
vm create --force || { echo "FAIL: create"; exit 1; }

# Test 2: Status (should not prompt)
vm status || { echo "FAIL: status"; exit 1; }

# Test 3: List (should not prompt)
vm list || { echo "FAIL: list"; exit 1; }

# Test 4: Exec (should not prompt)
vm exec "echo 'test'" || { echo "FAIL: exec"; exit 1; }

# Test 5: SSH with command (should not block)
timeout 10 vm ssh --command "echo 'test'" || { echo "FAIL: ssh"; exit 1; }

# Test 6: Destroy without confirmation
vm destroy --force || { echo "FAIL: destroy"; exit 1; }

echo "All non-interactive tests passed"
```

**Run and record:**
- [ ] All commands work without prompts: (Yes/No): _____
- [ ] No hanging/timeout: (Yes/No): _____
- [ ] Appropriate for CI: (Yes/No): _____

---

## Success Criteria

### Critical (Must Pass)

- [ ] ✅ Can install in CI without prompts
- [ ] ✅ Works in non-TTY environments
- [ ] ✅ All commands support `--force` / `--non-interactive`
- [ ] ✅ No color codes breaking parsers (or auto-detect NO_COLOR)
- [ ] ✅ Exit codes are correct (0 = success, non-zero = failure)
- [ ] ✅ Cleanup works without confirmation in CI mode
- [ ] ✅ Total workflow time < 10 minutes

### Important (Should Pass)

- [ ] Respects standard CI environment variables
- [ ] Works with Docker-in-Docker
- [ ] Logs are parseable (JSON option?)
- [ ] No prompts even on errors
- [ ] Resource limits respected

### Nice to Have

- [ ] GitHub Actions integration example
- [ ] GitLab CI template
- [ ] Jenkins pipeline example
- [ ] Caching support for faster CI runs

---

## Environment Variables Testing

Test support for standard CI environment variables:

```bash
# Test each variable's effect
CI=true vm create                # Should auto-enable non-interactive
NO_COLOR=1 vm list               # Should disable colors
VM_NO_PROMPT=true vm destroy     # Should skip confirmations
FORCE_COLOR=0 vm status          # Should respect color preference
TERM=dumb vm ssh                 # Should work without TTY features
```

**Record:**
| Variable | Expected Behavior | Actual Behavior | Status |
|----------|-------------------|-----------------|--------|
| `CI=true` | Non-interactive mode | _____ | ⬜ |
| `NO_COLOR=1` | No ANSI colors | _____ | ⬜ |
| `VM_NO_PROMPT=true` | No prompts | _____ | ⬜ |
| `TERM=dumb` | Basic terminal support | _____ | ⬜ |

---

## Integration Test Matrix

Test on multiple CI platforms:

### GitHub Actions
- [ ] Ubuntu 22.04 runner
- [ ] macOS runner
- [ ] Self-hosted runner with Docker

### GitLab CI
- [ ] Docker executor
- [ ] Shell executor
- [ ] Kubernetes executor

### Jenkins
- [ ] Docker agent
- [ ] VM agent

### Generic Docker
- [ ] Alpine-based image
- [ ] Ubuntu-based image
- [ ] Rust official image

**Record results:**
```markdown
| Platform | Status | Time | Notes |
|----------|--------|------|-------|
| GitHub Actions | ⬜ | _____ | _____ |
| GitLab CI | ⬜ | _____ | _____ |
| Jenkins | ⬜ | _____ | _____ |
| Docker | ⬜ | _____ | _____ |
```

---

## Error Handling in CI

Test that errors fail fast and clearly:

```bash
# These should fail with clear errors and non-zero exit
vm ssh nonexistent-vm ; echo "Exit code: $?"
vm destroy nonexistent-vm ; echo "Exit code: $?"
vm create --invalid-flag ; echo "Exit code: $?"
```

**Expected:**
- Clear error message
- Exit code != 0
- No stack traces (unless debug mode)
- Suggested fix in error message

**Record:**
- [ ] Errors have non-zero exit codes: (Yes/No): _____
- [ ] Error messages are parseable: (Yes/No): _____
- [ ] No leaked secrets in error logs: (Yes/No): _____

---

## Performance Benchmarks

**Target Times (CI Environment):**

| Operation | Target | Actual | Status |
|-----------|--------|--------|--------|
| Install VM (cached Rust) | < 3 min | _____ | ⬜ |
| Install VM (cold) | < 10 min | _____ | ⬜ |
| Create VM | < 2 min | _____ | ⬜ |
| SSH + Execute | < 30 sec | _____ | ⬜ |
| Destroy VM | < 30 sec | _____ | ⬜ |
| **Total Workflow** | **< 10 min** | **_____** | ⬜ |

---

## Caching Strategies

Test if caching improves CI performance:

### Cargo Cache
```yaml
# GitHub Actions
- uses: actions/cache@v3
  with:
    path: ~/.cargo
    key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
```

**Record:**
- Cold install time: _____ minutes
- Cached install time: _____ minutes
- Improvement: _____% faster

### VM Tool Binary Cache
```yaml
# Cache the installed binary
- uses: actions/cache@v3
  with:
    path: ~/.cargo/bin/vm
    key: vm-${{ hashFiles('rust/Cargo.lock') }}
```

**Record:**
- With binary cache: _____ seconds
- Worth caching: (Yes/No): _____

---

## Deliverables

### 1. Working CI Configurations

Provide tested, working examples:

- [ ] `.github/workflows/vm-test.yml` (GitHub Actions)
- [ ] `.gitlab-ci.yml` (GitLab)
- [ ] `Jenkinsfile` (Jenkins)
- [ ] `Dockerfile.ci` (Docker-based)

### 2. CI Integration Guide

Document discovered patterns:

```markdown
## Using VM Tool in CI/CD

### Quick Start (GitHub Actions)
[working example]

### Environment Variables
- `CI=true` - Auto-enable non-interactive mode
- `VM_NO_PROMPT=true` - Skip all confirmations
- `NO_COLOR=1` - Disable ANSI colors

### Common Issues
[list issues found during testing]

### Performance Tips
[caching strategies, parallel jobs, etc.]
```

### 3. Test Results

```markdown
## CI/CD Onboarding Test Results

**Tester:** [Name/Bot]
**Date:** [YYYY-MM-DD]
**Platform:** [CI Platform]
**Status:** [✅ PASS / ❌ FAIL / ⚠️ PARTIAL]

### Summary
[One paragraph summary]

### Tests Passed
- ✅ GitHub Actions: [time]
- ✅ GitLab CI: [time]
- ⚠️ Jenkins: [issues]
- ✅ Docker: [time]

### Issues Found
1. [Issue with workaround]
2. [Issue with workaround]

### Recommended Improvements
1. [Specific improvement]
2. [Specific improvement]
```

---

## Automation Score

Rate the CI/CD readiness:

### Criteria

| Aspect | Score (1-5) | Notes |
|--------|-------------|-------|
| Non-interactive support | _____ | |
| Standard CI vars | _____ | |
| Error handling | _____ | |
| Exit codes | _____ | |
| Performance | _____ | |
| Documentation | _____ | |
| **Overall** | **_____** | |

**Overall Automation Readiness:** _____ / 10

---

## Real-World CI Scenarios

### Scenario 1: Pull Request Testing
```yaml
# Run tests in VM for each PR
on: [pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo install vm
      - run: vm create --force
      - run: vm exec "npm test"
      - run: vm destroy --force
```

### Scenario 2: Multi-Environment Deploy
```yaml
# Deploy to dev, staging, prod VMs
strategy:
  matrix:
    env: [dev, staging, prod]
steps:
  - run: vm create --instance ${{ matrix.env }}
  - run: vm exec ${{ matrix.env }} "./deploy.sh"
```

### Scenario 3: Integration Testing
```yaml
# Spin up full stack for integration tests
steps:
  - run: vm create --instance backend
  - run: vm create --instance frontend
  - run: vm create --instance db
  - run: ./run-integration-tests.sh
  - run: vm destroy --all --force
```

**Test and record which scenarios work:**
- [ ] PR testing: (Yes/No): _____
- [ ] Multi-environment: (Yes/No): _____
- [ ] Integration tests: (Yes/No): _____

---

## Success Metrics

**This test PASSES if:**
- ✅ All 4 main tests pass
- ✅ Works on ≥2 CI platforms
- ✅ Total workflow < 10 minutes
- ✅ Zero manual intervention needed
- ✅ Automation score ≥ 8/10

**This test is ACCEPTABLE if:**
- ⚠️ 3/4 tests pass
- ⚠️ Works on 1 CI platform
- ⚠️ Workflow 10-15 minutes
- ⚠️ Minor manual steps needed
- ⚠️ Automation score 6-7/10

**This test FAILS if:**
- ❌ <3 tests pass
- ❌ Requires prompts in CI
- ❌ Workflow > 15 minutes
- ❌ Cannot complete without intervention
- ❌ Automation score < 6/10

---

## Notes for CI Testing

**Best Practices:**
- Use fresh runners/containers for each test
- Test with minimal cached state
- Verify cleanup to avoid resource leaks
- Test both success and failure paths
- Ensure secrets aren't logged

**Common CI Gotchas:**
- TTY assumptions (colors, prompts, interactive commands)
- Docker-in-Docker permissions
- Resource limits (memory, CPU, time)
- Network restrictions
- Filesystem permissions

---

## Related Tests

This is **Path 3** of 3 onboarding tests:

- 🔄 **Proposal 15:** Complete Beginner (manual, no prerequisites)
- 🔄 **Proposal 16:** Quick Start Developer (manual, fast path)
- ✅ **Proposal 17:** CI/CD Automated (this test - zero-touch)

**Cross-test analysis:**
- Compare manual vs automated times
- Identify features that don't work in CI
- Find prompts that should have `--force` flags
- Validate error messages work without TTY
