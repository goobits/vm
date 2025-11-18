# Dockerfile.vibe Optimization - Verification Tests

This document provides comprehensive test procedures to verify the optimized Dockerfile works correctly and achieves the expected performance improvements.

## Prerequisites

- Docker 18.09+ with BuildKit support
- At least 10GB free disk space
- `time` command available (for timing measurements)

## Test Suite

### Test 1: BuildKit Support Check

**Purpose:** Verify Docker supports BuildKit features

```bash
# Check Docker version
docker --version

# Should be 18.09 or higher
# Example output: Docker version 24.0.7, build afdd53b

# Check if BuildKit is available
docker buildx version

# Should show buildx version
# Example output: github.com/docker/buildx v0.12.1
```

**Expected Result:** Docker 18.09+ and buildx available

**If Failed:**
```bash
# Upgrade Docker
# Ubuntu/Debian:
sudo apt-get update && sudo apt-get install docker-ce docker-ce-cli

# macOS:
# Download latest Docker Desktop from docker.com

# Enable BuildKit
export DOCKER_BUILDKIT=1
echo 'export DOCKER_BUILDKIT=1' >> ~/.bashrc
```

---

### Test 2: Cold Build Time (No Cache)

**Purpose:** Measure build time from scratch

```bash
# Clear all caches
docker builder prune -a -f

# Build original (if exists)
if [ -f Dockerfile.vibe ]; then
  echo "Building original Dockerfile..."
  time docker build -t vibe-box-original -f Dockerfile.vibe .
fi

# Clear cache again
docker builder prune -a -f

# Build optimized
echo "Building optimized Dockerfile..."
time DOCKER_BUILDKIT=1 docker build -t vibe-box-optimized -f Dockerfile.vibe.optimized .
```

**Expected Results:**
- Original: 18-25 minutes
- Optimized: 10-15 minutes
- **Improvement: 40-50% faster**

**Record Results:**
```
Original build time:  _____ seconds
Optimized build time: _____ seconds
Time saved:           _____ seconds
Percentage improvement: ____%
```

---

### Test 3: Warm Build Time (Full Cache)

**Purpose:** Measure rebuild time with cache

```bash
# Build once to populate cache
DOCKER_BUILDKIT=1 docker build -t vibe-box-test1 -f Dockerfile.vibe.optimized .

# Rebuild immediately (warm cache)
echo "Testing warm build..."
time DOCKER_BUILDKIT=1 docker build -t vibe-box-test2 -f Dockerfile.vibe.optimized .
```

**Expected Results:**
- Build time: 30-60 seconds
- Most layers should show "CACHED"
- Download steps should be instant (cache mounts)

**Success Criteria:**
- Build completes in under 2 minutes
- Output shows extensive cache usage

---

### Test 4: Incremental Build (NPM Package Change)

**Purpose:** Test cache effectiveness when changing Node packages

```bash
# Create modified Dockerfile
cp Dockerfile.vibe.optimized Dockerfile.vibe.test

# Add a new NPM package
sed -i '/npm-check-updates/a \    pnpm' Dockerfile.vibe.test

# Build with change
echo "Testing incremental build (NPM package added)..."
time DOCKER_BUILDKIT=1 docker build -t vibe-box-test-npm -f Dockerfile.vibe.test .

# Cleanup
rm Dockerfile.vibe.test
```

**Expected Results:**
- Only node-builder and final stages rebuild
- python-builder and rust-builder stages cached
- Build time: 2-4 minutes (vs 8-10 min original)

---

### Test 5: Incremental Build (Cargo Tool Change)

**Purpose:** Test cache effectiveness when changing Rust tools

```bash
# Create modified Dockerfile
cp Dockerfile.vibe.optimized Dockerfile.vibe.test

# Add a new Cargo tool
sed -i '/cargo-outdated/a \    cargo-expand' Dockerfile.vibe.test

# Build with change
echo "Testing incremental build (Cargo tool added)..."
time DOCKER_BUILDKIT=1 docker build -t vibe-box-test-cargo -f Dockerfile.vibe.test .

# Cleanup
rm Dockerfile.vibe.test
```

**Expected Results:**
- Only rust-builder and final stages rebuild
- node-builder and python-builder stages cached
- Build time: 3-5 minutes (vs 10-12 min original)

---

### Test 6: Functionality Verification

**Purpose:** Verify all tools work correctly in the container

```bash
# Start container
docker run -it --rm vibe-box-optimized bash << 'EOF'

echo "=== Testing Node.js ==="
node --version || { echo "FAIL: Node not found"; exit 1; }
npm --version || { echo "FAIL: NPM not found"; exit 1; }
echo "Node.js version: $(node --version)"
echo "NPM version: $(npm --version)"

echo ""
echo "=== Testing NPM Global Packages ==="
claude --version || { echo "FAIL: claude-code not installed"; exit 1; }
gemini --version 2>&1 | head -1 || { echo "FAIL: gemini-cli not installed"; exit 1; }
npx playwright --version || { echo "FAIL: Playwright not installed"; exit 1; }
prettier --version || { echo "FAIL: Prettier not installed"; exit 1; }
tsc --version || { echo "FAIL: TypeScript not installed"; exit 1; }
echo "All NPM packages installed ✓"

echo ""
echo "=== Testing Python ==="
python --version || { echo "FAIL: Python not found"; exit 1; }
python3 --version || { echo "FAIL: Python3 not found"; exit 1; }
pip3 --version || { echo "FAIL: pip not found"; exit 1; }
echo "Python version: $(python --version)"
echo "Pip version: $(pip3 --version)"

echo ""
echo "=== Testing Python Packages ==="
ansible --version | head -1 || { echo "FAIL: Ansible not installed"; exit 1; }
python -c "import playwright" || { echo "FAIL: Playwright Python package not installed"; exit 1; }
pytest --version || { echo "FAIL: pytest not installed"; exit 1; }
echo "All Python packages installed ✓"

echo ""
echo "=== Testing Rust ==="
rustc --version || { echo "FAIL: Rust not found"; exit 1; }
cargo --version || { echo "FAIL: Cargo not found"; exit 1; }
echo "Rust version: $(rustc --version)"
echo "Cargo version: $(cargo --version)"

echo ""
echo "=== Testing Rust Components ==="
rustfmt --version || { echo "FAIL: rustfmt not installed"; exit 1; }
cargo clippy --version || { echo "FAIL: clippy not installed"; exit 1; }
rust-analyzer --version || { echo "FAIL: rust-analyzer not installed"; exit 1; }
echo "All Rust components installed ✓"

echo ""
echo "=== Testing Cargo Tools ==="
cargo watch --version || { echo "FAIL: cargo-watch not installed"; exit 1; }
cargo upgrade --version || { echo "FAIL: cargo-edit not installed"; exit 1; }
cargo audit --version || { echo "FAIL: cargo-audit not installed"; exit 1; }
cargo outdated --version || { echo "FAIL: cargo-outdated not installed"; exit 1; }
echo "All Cargo tools installed ✓"

echo ""
echo "=== Testing System Tools ==="
tree --version || { echo "FAIL: tree not installed"; exit 1; }
rg --version || { echo "FAIL: ripgrep not installed"; exit 1; }
jq --version || { echo "FAIL: jq not installed"; exit 1; }
tmux -V || { echo "FAIL: tmux not installed"; exit 1; }
git --version || { echo "FAIL: git not installed"; exit 1; }
echo "All system tools installed ✓"

echo ""
echo "=== Testing Playwright Browser ==="
# Check if Chromium is installed
ls ~/.cache/ms-playwright/chromium-* > /dev/null 2>&1 || { echo "FAIL: Chromium browser not installed"; exit 1; }
echo "Playwright Chromium installed ✓"

echo ""
echo "=== All Tests Passed ==="
exit 0
EOF

# Check exit code
if [ $? -eq 0 ]; then
  echo ""
  echo "✅ All functionality tests PASSED"
else
  echo ""
  echo "❌ Some functionality tests FAILED"
  exit 1
fi
```

**Expected Results:**
- All tools found and working
- All version checks pass
- Exit code 0 (success)

---

### Test 7: Image Size Comparison

**Purpose:** Verify image sizes are similar

```bash
echo "=== Image Size Comparison ==="
docker images | grep -E "REPOSITORY|vibe-box" | grep -v "<none>"

# Get specific sizes
ORIGINAL_SIZE=$(docker images vibe-box-original --format "{{.Size}}" 2>/dev/null || echo "N/A")
OPTIMIZED_SIZE=$(docker images vibe-box-optimized --format "{{.Size}}")

echo ""
echo "Original image size:  $ORIGINAL_SIZE"
echo "Optimized image size: $OPTIMIZED_SIZE"
echo ""
echo "Note: Sizes should be similar (~2-3 GB)"
echo "Optimization is in BUILD TIME, not final size"
```

**Expected Results:**
- Both images ~2-3 GB
- Size difference < 10%
- Optimization is build time, not size

---

### Test 8: Cache Mount Effectiveness

**Purpose:** Verify cache mounts are working

```bash
# Build with cache stats
echo "=== Testing Cache Mount Effectiveness ==="

# First build (populates cache)
docker builder prune -a -f
DOCKER_BUILDKIT=1 docker build \
  --progress=plain \
  -t vibe-test-cache1 \
  -f Dockerfile.vibe.optimized \
  . 2>&1 | tee build1.log

# Check for cache mount messages
grep -q "mount type=cache" build1.log && echo "✓ Cache mounts detected in build" || echo "✗ No cache mounts found"

# Second build (uses cache)
DOCKER_BUILDKIT=1 docker build \
  --progress=plain \
  -t vibe-test-cache2 \
  -f Dockerfile.vibe.optimized \
  . 2>&1 | tee build2.log

# Count cached layers
CACHED_LAYERS=$(grep -c "CACHED" build2.log || echo 0)
echo "Cached layers in second build: $CACHED_LAYERS"

# Cleanup
rm build1.log build2.log

if [ $CACHED_LAYERS -gt 10 ]; then
  echo "✅ Cache mounts working effectively"
else
  echo "❌ Cache mounts may not be working properly"
fi
```

**Expected Results:**
- First build populates cache
- Second build shows many "CACHED" layers
- 15+ cached layers expected

---

### Test 9: Parallel Build Verification

**Purpose:** Verify stages build in parallel

```bash
# Build with detailed timing
echo "=== Testing Parallel Stage Building ==="

DOCKER_BUILDKIT=1 docker buildx build \
  --progress=plain \
  -t vibe-test-parallel \
  -f Dockerfile.vibe.optimized \
  . 2>&1 | tee parallel.log

# Check for parallel execution indicators
echo ""
echo "Stages found in build:"
grep "FROM.*AS" Dockerfile.vibe.optimized

echo ""
echo "Build log should show multiple stages executing:"
grep -E "(node-builder|python-builder|rust-builder)" parallel.log | head -20

# Cleanup
rm parallel.log
```

**Expected Results:**
- Multiple "FROM ... AS" stages visible
- Build log shows interleaved output from different stages
- Indicates parallel execution

---

### Test 10: Environment Variables

**Purpose:** Verify all environment variables are set

```bash
docker run -it --rm vibe-box-optimized bash -c '
echo "=== Environment Variables ==="
echo "NVM_DIR: $NVM_DIR"
echo "CARGO_HOME: $CARGO_HOME"
echo "RUSTUP_HOME: $RUSTUP_HOME"
echo "NODE_ENV: $NODE_ENV"
echo "PLAYWRIGHT_BROWSERS_PATH: $PLAYWRIGHT_BROWSERS_PATH"
echo "PATH: $PATH"
echo ""

# Verify critical env vars are set
[ -n "$NVM_DIR" ] && echo "✓ NVM_DIR set" || echo "✗ NVM_DIR not set"
[ -n "$CARGO_HOME" ] && echo "✓ CARGO_HOME set" || echo "✗ CARGO_HOME not set"
[ -n "$RUSTUP_HOME" ] && echo "✓ RUSTUP_HOME set" || echo "✗ RUSTUP_HOME not set"

# Verify tools are in PATH
command -v node && echo "✓ node in PATH" || echo "✗ node not in PATH"
command -v cargo && echo "✓ cargo in PATH" || echo "✗ cargo not in PATH"
command -v python3 && echo "✓ python3 in PATH" || echo "✗ python3 in PATH"
'
```

**Expected Results:**
- All environment variables set
- All tools accessible via PATH
- No "not set" or "not in PATH" messages

---

### Test 11: User Permissions

**Purpose:** Verify proper file ownership

```bash
docker run -it --rm vibe-box-optimized bash -c '
echo "=== User and Permissions ==="
echo "Current user: $(whoami)"
echo "User ID: $(id -u)"
echo "Group ID: $(id -g)"
echo ""

# Check ownership of key directories
ls -la ~ | head -10
echo ""

# Verify write permissions
touch /workspace/test.txt && rm /workspace/test.txt && echo "✓ Can write to /workspace" || echo "✗ Cannot write to /workspace"
touch ~/.test.txt && rm ~/.test.txt && echo "✓ Can write to home directory" || echo "✗ Cannot write to home"

# Check .nvm ownership
[ -d ~/.nvm ] && ls -ld ~/.nvm | grep developer && echo "✓ .nvm owned by developer" || echo "✗ .nvm ownership issue"

# Check .cargo ownership
[ -d ~/.cargo ] && ls -ld ~/.cargo | grep developer && echo "✓ .cargo owned by developer" || echo "✗ .cargo ownership issue"
'
```

**Expected Results:**
- User: developer
- UID: 1000, GID: 1000
- Write access to /workspace and ~
- Proper ownership of .nvm and .cargo

---

### Test 12: Healthcheck

**Purpose:** Verify container healthcheck works

```bash
# Start container in background
CONTAINER_ID=$(docker run -d vibe-box-optimized sleep 300)

# Wait for healthcheck
echo "Waiting for healthcheck..."
sleep 5

# Check health status
HEALTH_STATUS=$(docker inspect --format='{{.State.Health.Status}}' $CONTAINER_ID)
echo "Health status: $HEALTH_STATUS"

# View healthcheck logs
docker inspect --format='{{range .State.Health.Log}}{{.Output}}{{end}}' $CONTAINER_ID

# Cleanup
docker stop $CONTAINER_ID
docker rm $CONTAINER_ID

if [ "$HEALTH_STATUS" = "healthy" ]; then
  echo "✅ Healthcheck PASSED"
else
  echo "❌ Healthcheck FAILED"
fi
```

**Expected Results:**
- Health status: "healthy"
- Healthcheck logs show successful checks

---

## Test Results Summary

After running all tests, fill in this summary:

```
┌─────────────────────────────────────────────────────────────┐
│           Dockerfile Optimization Test Results              │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│ Test 1: BuildKit Support          [ PASS / FAIL ]          │
│ Test 2: Cold Build Time           [ PASS / FAIL ]          │
│   - Original:  _____ min                                    │
│   - Optimized: _____ min                                    │
│   - Savings:   _____ min (___%)                            │
│                                                             │
│ Test 3: Warm Build Time           [ PASS / FAIL ]          │
│   - Time: _____ seconds                                     │
│                                                             │
│ Test 4: Incremental (NPM)         [ PASS / FAIL ]          │
│   - Time: _____ minutes                                     │
│                                                             │
│ Test 5: Incremental (Cargo)       [ PASS / FAIL ]          │
│   - Time: _____ minutes                                     │
│                                                             │
│ Test 6: Functionality             [ PASS / FAIL ]          │
│   - Node.js:        ✓ / ✗                                  │
│   - Python:         ✓ / ✗                                  │
│   - Rust:           ✓ / ✗                                  │
│   - All packages:   ✓ / ✗                                  │
│                                                             │
│ Test 7: Image Size                [ PASS / FAIL ]          │
│   - Original:  _____ GB                                     │
│   - Optimized: _____ GB                                     │
│                                                             │
│ Test 8: Cache Mounts              [ PASS / FAIL ]          │
│ Test 9: Parallel Builds           [ PASS / FAIL ]          │
│ Test 10: Environment Vars         [ PASS / FAIL ]          │
│ Test 11: User Permissions         [ PASS / FAIL ]          │
│ Test 12: Healthcheck              [ PASS / FAIL ]          │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ Overall Result:                   [ PASS / FAIL ]          │
└─────────────────────────────────────────────────────────────┘
```

## Quick Test Script

Run all tests automatically:

```bash
#!/bin/bash
# Save as: run-all-tests.sh

set -e

echo "=== Running Dockerfile Optimization Test Suite ==="
echo ""

# Test 1: BuildKit
echo "Test 1: BuildKit Support"
docker --version
docker buildx version
echo "✅ Test 1 passed"
echo ""

# Test 2: Build optimized
echo "Test 2: Building optimized image"
docker builder prune -a -f
time DOCKER_BUILDKIT=1 docker build -t vibe-box-optimized -f Dockerfile.vibe.optimized .
echo "✅ Test 2 passed"
echo ""

# Test 3: Warm build
echo "Test 3: Warm build test"
time DOCKER_BUILDKIT=1 docker build -t vibe-box-test -f Dockerfile.vibe.optimized .
echo "✅ Test 3 passed"
echo ""

# Test 6: Functionality
echo "Test 6: Functionality verification"
docker run -it --rm vibe-box-optimized bash -c '
  node --version && npm --version &&
  python --version && pip3 --version &&
  rustc --version && cargo --version &&
  claude --version && echo "All tools working"
'
echo "✅ Test 6 passed"
echo ""

# Test 7: Image size
echo "Test 7: Image size check"
docker images | grep vibe-box
echo "✅ Test 7 passed"
echo ""

echo "=== All Tests Completed Successfully ==="
```

## Troubleshooting

### Issue: Tests taking too long

**Solution:** Tests involve full Docker builds which can take 10-30 minutes total. Run tests in order and skip Tests 4-5 if time-constrained.

### Issue: BuildKit not available

**Solution:**
```bash
export DOCKER_BUILDKIT=1
# Or upgrade Docker to 18.09+
```

### Issue: Permission denied errors

**Solution:**
```bash
# Add user to docker group
sudo usermod -aG docker $USER
newgrp docker
```

### Issue: Out of disk space

**Solution:**
```bash
# Clean up Docker resources
docker system prune -a
docker builder prune -a
```

## Success Criteria

The optimization is successful if:

- ✅ All 12 tests pass
- ✅ Cold build time reduced by 30%+
- ✅ Warm build time under 2 minutes
- ✅ All tools function correctly
- ✅ Image size similar to original
- ✅ Cache mounts working

## Next Steps After Testing

1. **If all tests pass:** Proceed with deployment
2. **If some tests fail:** Review failed tests and fix issues
3. **Document results:** Record build times and improvements
4. **Share findings:** Update team on optimization benefits
