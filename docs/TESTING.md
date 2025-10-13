# Testing Strategy

This document explains the testing infrastructure and how to run different types of tests.

## Test Categories

### 1. Unit Tests (Fast, No Dependencies)
Pure logic tests that don't require external services or network access.

```bash
make test-unit
# or
cd rust && cargo test --workspace --lib -- --test-threads=10
```

**What's tested:**
- Business logic
- Data structures
- Utility functions
- Validation logic
- Pure computation

**No Keychain prompts** ✅

---

### 2. Integration Tests (Docker Required)
Tests that verify cross-crate functionality and require Docker.

```bash
make test-integration
# or
cd rust && cargo test --workspace --test '*' --features integration -- --test-threads=2
```

**What's tested:**
- VM lifecycle operations
- Container creation/destruction
- SSH connections
- Configuration workflows
- Provider implementations

**No Keychain prompts** ✅

---

### 3. Network Tests (TLS/Keychain Required)
Tests that connect to real upstream registries (PyPI, npm, crates.io).

```bash
make test-network
# or
cd rust && cargo test --workspace --features network-tests -- --test-threads=2
```

**What's tested:**
- Package registry connectivity
- Upstream caching logic
- TLS certificate handling
- Network error recovery

**⚠️ May prompt for Keychain access** on macOS

**When to run:** Only when testing `vm-package-server` network functionality. Not needed for VM tool development.

---

## Default Test Command

```bash
make test
```

Runs unit + integration tests (no network). This is what you want for normal development.

---

## Why Network Tests are Separate

The `vm-package-server` package is a caching proxy for package registries. Its tests create an `UpstreamClient` which:

1. Initializes `reqwest::Client` with TLS support
2. On macOS, this triggers Keychain access to system root certificates
3. You'll see: **"cargo wants to access your keychain"**

**You don't need these tests** unless you're modifying the package server's upstream connectivity logic.

---

## Skipping Tests

### Skip Integration Tests
```bash
SKIP_INTEGRATION_TESTS=1 make test
```

### Skip Network Tests (Default)
Network tests are **already skipped by default** unless you run `make test-network`.

### Skip Specific Package
```bash
cargo test --workspace --exclude vm-package-server
```

---

## CI/CD Recommendations

```yaml
# Fast feedback loop
- name: Unit tests
  run: make test-unit

# Full validation (no network)
- name: Integration tests
  run: make test-integration

# Optional: Network tests (if needed)
- name: Network tests
  run: make test-network
  if: github.event_name == 'schedule' # Run weekly
```

---

## Troubleshooting

### Keychain Prompt on macOS

**Why:** Tests are trying to access TLS certificates for HTTPS connections.

**Solution:**
1. Click "Always Allow" for cargo/rust test processes
2. Or skip network tests: don't run `make test-network`

### Tests Timeout

**Parallel execution:**
- Unit tests: `--test-threads=10` (fast, can run many in parallel)
- Integration tests: `--test-threads=2` (Docker containers, limit parallelism)
- Network tests: `--test-threads=2` (avoid overwhelming upstream servers)

**Solution:**
```bash
# Run serially for debugging
cargo test -- --test-threads=1 --nocapture
```

### Docker Not Available

Integration tests will skip gracefully if Docker isn't running.

```bash
# Check Docker status
docker info
```

---

## Test Structure

```
rust/
├── vm/tests/           # Integration tests for VM operations
├── vm-config/          # Config system (unit + integration)
├── vm-provider/        # Provider implementations (unit + integration)
├── vm-temp/            # Temp VM operations (unit + integration)
├── vm-package-server/  # Package registry (unit + NETWORK tests)
└── */tests/            # Package-specific integration tests
```

---

## Quick Reference

| Command | What It Runs | Keychain Prompt? | Duration |
|---------|-------------|-----------------|----------|
| `make test` | Unit + Integration | ❌ No | ~30s |
| `make test-unit` | Unit only | ❌ No | ~5s |
| `make test-integration` | Integration only | ❌ No | ~20s |
| `make test-network` | Network tests | ⚠️ Yes (macOS) | ~60s |

---

## For Contributors

**Daily development:** Use `make test` (no Keychain prompts)

**Before PR:** Run `make quality-gates` (includes all non-network tests)

**Package server work:** Run `make test-network` after clicking "Always Allow"
