# AI Agent Setup Instructions - VM Tool

## Mission
Set up the VM Tool development environment from scratch, run all tests, verify the build system works, and report back with a comprehensive status report.

---

## Phase 1: Prerequisites Check (10 minutes)

### 1.1 Verify System Requirements

```bash
# Check operating system
uname -a

# Check if Docker is installed and running
docker --version
docker ps

# Check if Rust is installed
rustc --version
cargo --version

# Expected: Rust 1.63.0 or higher (we're on 1.90.0+)
```

**Report back:**
- OS and version
- Docker version and status
- Rust/Cargo version
- Any missing prerequisites

### 1.2 Install Missing Prerequisites

**If Rust is missing:**
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

**If Docker is missing:**
- macOS: Install Docker Desktop
- Linux: Follow Docker installation guide for your distro
  - **Note:** On Linux, you may need to run Docker commands with `sudo` OR add your user to the `docker` group: `sudo usermod -aG docker $USER` (requires logout/login)
- Verify with `docker run hello-world` (or `sudo docker run hello-world` on Linux)

---

## Phase 2: Repository Setup (5 minutes)

### 2.1 Clone and Navigate

```bash
# If not already cloned, clone the repository
git clone <repository-url>
cd vm

# Verify you're in the right directory
ls -la
# Should see: rust/, Makefile, README.md, CLAUDE.md, install.sh
```

### 2.2 Check Repository State

```bash
# Check current branch
git branch -a

# Check git status
git status

# Check recent commits
git log --oneline -10
```

**Report back:**
- Current branch name
- Any uncommitted changes
- Last 5 commit messages

---

## Phase 3: Build System Verification (15 minutes)

### 3.1 Initial Build Test

```bash
cd rust

# Check compilation without building
cargo check --workspace

# Report any errors immediately if this fails
```

**Expected:** Should complete without errors. If errors occur, report them immediately.

### 3.2 Full Build

```bash
# Build all packages in debug mode
cargo build --workspace

# Report build time and any warnings
```

**Expected:** Build completes in 2-5 minutes on modern hardware.

### 3.3 Release Build Test

```bash
# Build in release mode (takes longer)
cargo build --workspace --release
```

**Report back:**
- Total build time
- Number of packages compiled
- Binary size of `target/release/vm`
- Any warnings or errors

---

## Phase 4: Test Suite Execution (30 minutes)

### 4.1 Run Unit Tests

```bash
# Run all unit tests (library tests only)
cargo test --workspace --lib

# Count results
cargo test --workspace --lib 2>&1 | grep "test result:"
```

**Expected:** All tests should pass. Current count: ~380+ tests.

**Note:** Some package server tests require external tools:
- PyPI tests require Python 3 + setuptools
- NPM tests require Node.js/npm

These tests will automatically skip if the tools are not available. This is normal and not an error.

### 4.2 Run Integration Tests

```bash
# Run all integration tests
cargo test --workspace --test '*'

# Run specific critical integration tests
cargo test --package vm --test vm_ops
cargo test --package vm --test config_cli_tests
cargo test --package vm --test pkg_cli_tests
```

**Expected:** All integration tests pass. Some may be skipped if Docker isn't available.

### 4.3 Run Full Test Suite

```bash
# Run everything (unit + integration + doc tests)
cargo test --workspace 2>&1 | tee test-results.txt

# Count total tests
grep "test result:" test-results.txt | wc -l

# Check for failures
grep "FAILED" test-results.txt || echo "All tests passed"
```

**Report back:**
- Total number of tests run
- Number of tests passed
- Number of tests failed (should be 0)
- Number of tests skipped
- Total test execution time
- Contents of any failures

---

## Phase 5: Quality Gates (20 minutes)

### 5.1 Code Formatting Check

```bash
# Check if code is properly formatted
cargo fmt --all -- --check

# If not formatted, report which files need formatting
```

**Expected:** Code should already be formatted. If not, report the files.

### 5.2 Linting with Clippy

```bash
# Run clippy with warnings as errors
cargo clippy --workspace -- -D warnings

# Report any clippy warnings
```

**Expected:** Zero warnings. Report any that appear.

### 5.3 Dead Code Detection

```bash
# Check for dead code
cargo clippy --workspace -- -D dead_code

# Report any dead code findings
```

**Expected:** Zero dead code warnings (after recent cleanup).

### 5.4 Dependency Audit

```bash
# Check for unused dependencies (if cargo-machete is installed)
cargo machete 2>&1 || echo "cargo-machete not installed, skipping"

# If not installed, install it:
# cargo install cargo-machete
# Then run: cargo machete
```

**Expected:** "didn't find any unused dependencies"

---

## Phase 6: Makefile Commands (10 minutes)

### 6.1 Test Makefile Targets

```bash
cd ..  # Go back to project root
pwd    # Should be in /workspace or similar

# List available make targets
make help

# Test individual targets
make fmt
make clippy
make test

# Test the full quality gate
make check
```

**Report back:**
- List of all available make targets
- Result of each command (success/failure)
- Any errors encountered

---

## Phase 7: Installation Test (10 minutes)

### 7.1 Test Install Script

```bash
# Test the install script (build from source)
./install.sh --build-from-source

# Verify installation
which vm
vm --version
```

**Expected:** VM tool is installed and in PATH.

### 7.2 Basic Functionality Test

```bash
# Test basic commands
vm --help
vm config --help
vm create --help

# Test init command
cd /tmp
mkdir test-vm-project
cd test-vm-project
vm init
cat vm.yaml
```

**Report back:**
- VM version installed
- Whether all help commands work
- Contents of generated vm.yaml

---

## Phase 8: Docker Integration Test (15 minutes)

### 8.1 Create Test VM

```bash
# Create a simple test project
cd /tmp
mkdir vm-test-$(date +%s)
cd vm-test-*
echo "# Test Project" > README.md

# Initialize vm config
vm init

# Try to create a VM (if Docker is running)
vm create --force

# Check VM status
vm status
vm list
```

**Expected:** VM creates successfully if Docker is available.

### 8.2 Test VM Operations

```bash
# Stop the VM
vm stop

# Start it again
vm start

# SSH into it (exit immediately)
vm ssh -c "echo 'VM connection test successful'"

# Clean up
vm destroy --force
```

**Report back:**
- Whether VM created successfully
- Whether all operations worked
- Any errors encountered
- Docker container names created

---

## Phase 9: Documentation Review (10 minutes)

### 9.1 Check Documentation Exists

```bash
cd /workspace

# List all documentation files
find . -name "*.md" -type f | grep -v node_modules | grep -v target | sort

# Verify key docs exist
ls -la README.md
ls -la CLAUDE.md
ls -la CONTRIBUTING.md
ls -la rust/ARCHITECTURE.md 2>/dev/null || echo "ARCHITECTURE.md not found"
```

### 9.2 Read Key Documentation

Read and summarize:
1. README.md - User-facing documentation
2. CLAUDE.md - Developer documentation
3. CONTRIBUTING.md - Contribution guidelines

**Report back:**
- List of all markdown files found
- Which key documentation files exist
- Brief summary of what each covers
- Any missing or incomplete sections

---

## Phase 10: Comprehensive Status Report

Generate a final report with the following sections:

### 10.1 System Information
```
- OS: [name and version]
- Architecture: [x86_64, aarch64, etc.]
- Docker: [version and status]
- Rust: [version]
- Cargo: [version]
```

### 10.2 Build Results
```
- Debug build: [SUCCESS/FAILED] in [X minutes]
- Release build: [SUCCESS/FAILED] in [X minutes]
- Binary size: [X MB]
- Warnings: [count]
```

### 10.3 Test Results
```
- Total tests: [count]
- Passed: [count]
- Failed: [count]
- Skipped: [count]
- Execution time: [X seconds]
- Test coverage areas verified:
  - Unit tests: [status]
  - Integration tests: [status]
  - Doc tests: [status]
  - VM operations: [status]
```

### 10.4 Quality Gates
```
- Code formatting: [PASS/FAIL]
- Clippy linting: [PASS/FAIL with X warnings]
- Dead code check: [PASS/FAIL]
- Dependency audit: [PASS/FAIL]
- Make targets: [list which ones work]
```

### 10.5 Installation
```
- Install script: [SUCCESS/FAILED]
- Binary location: [path]
- Version: [X.Y.Z]
- Basic commands: [list which ones work]
```

### 10.6 Docker Integration
```
- VM creation: [SUCCESS/FAILED]
- VM operations: [list which worked]
- Cleanup: [SUCCESS/FAILED]
```

### 10.7 Documentation Status
```
- README.md: [EXISTS/MISSING] - [brief summary]
- CLAUDE.md: [EXISTS/MISSING] - [brief summary]
- CONTRIBUTING.md: [EXISTS/MISSING] - [brief summary]
- ARCHITECTURE.md: [EXISTS/MISSING] - [brief summary]
- Other docs: [list]
```

### 10.8 Issues Found
List any issues discovered:
```
1. [Issue description]
   - Severity: [High/Medium/Low]
   - Location: [file/command]
   - Error: [error message]
   - Suggested fix: [if applicable]

2. ...
```

### 10.9 Missing or Unclear Documentation
List anything that was confusing or missing:
```
1. [Topic/area]
   - What's missing: [description]
   - Why it matters: [explanation]
   - Suggested addition: [brief outline]

2. ...
```

### 10.10 Recommendations
Provide recommendations for:
- Improving setup process
- Clarifying documentation
- Fixing any issues found
- Making it easier for future developers

---

## Expected Timeline

- **Prerequisites Check**: 10 minutes
- **Repository Setup**: 5 minutes
- **Build Verification**: 15 minutes
- **Test Suite**: 30 minutes
- **Quality Gates**: 20 minutes
- **Makefile Commands**: 10 minutes
- **Installation Test**: 10 minutes
- **Docker Integration**: 15 minutes
- **Documentation Review**: 10 minutes
- **Report Generation**: 15 minutes

**Total: ~2.5 hours**

---

## Success Criteria

You've successfully completed the setup verification if:

✅ All prerequisites are installed
✅ Repository cloned and at correct commit
✅ Debug build completes without errors
✅ Release build completes without errors
✅ All tests pass (or only skip due to missing Docker)
✅ All quality gates pass (fmt, clippy, dead code)
✅ Install script works
✅ Basic VM commands execute
✅ Documentation exists and is readable
✅ Comprehensive status report generated

---

## If Something Fails

**Don't panic!** Report the failure with:
1. Exact command that failed
2. Complete error message
3. Your environment details (OS, Rust version, etc.)
4. Steps you took before the failure
5. Any relevant logs or output

We'll help you debug and identify if it's:
- A documentation gap (needs clarification)
- A missing prerequisite (needs to be documented)
- An actual bug (needs to be fixed)
- An environment-specific issue (needs troubleshooting)

---

## After Completion

Once you've generated the final report:
1. Save it as `SETUP_VERIFICATION_REPORT.md`
2. Include all test outputs in a `logs/` directory
3. Highlight any issues found
4. Propose documentation improvements based on your experience
5. Report back with executive summary + full detailed report

**Good luck! 🚀**
