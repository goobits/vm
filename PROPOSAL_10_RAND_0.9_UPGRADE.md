# Proposal: Upgrade rand Crate to 0.9

**Status**: Draft
**Owner**: Infrastructure
**Target Release**: 2.2.0
**Priority**: Low

---

## Problem

The `rand` crate and its ecosystem have released major version 0.9, but we're currently on 0.8.5. This represents a major version jump with breaking API changes.

**Current versions:**
- `rand`: 0.8.5 → 0.9.2 (latest)
- `rand_core`: 0.6.4 → 0.9.3 (latest)
- `rand_chacha`: 0.3.1 → 0.9.0 (latest)
- `getrandom`: 0.2.16 → 0.3.3 (latest)

**Why upgrade?**
- Security: Latest version may have security fixes
- Performance: Improved performance in random number generation
- Ecosystem: Stay compatible with other crates that upgrade to 0.9
- Maintenance: Avoid technical debt from staying on old versions

---

## Current Usage Analysis

We use `rand` in **only one file**: `vm-auth-proxy/src/crypto.rs`

**Current usage:**
```rust
use rand::RngCore;  // Line 10

// Used via OsRng from aes_gcm crate:
OsRng.fill_bytes(&mut salt);        // Line 92
OsRng.fill_bytes(&mut token_bytes); // Line 99
```

**Key insight:** We use `OsRng` from `aes_gcm` crate, not from `rand` directly. This means our actual `rand` dependency usage is **minimal** - we only import the `RngCore` trait for type compatibility.

---

## Breaking Changes in rand 0.9

### 1. API Renamings (Not affecting us)
- `thread_rng()` → `rng()`
- `Rng::gen()` → `random()`
- `Rng::gen_range()` → `random_range()`

**Impact:** ✅ None - we don't use these functions

### 2. Trait Changes
- `RngCore` trait remains but may have minor changes
- New traits: `TryRngCore`, `TryCryptoRng`

**Impact:** ⚠️ Minimal - we only import `RngCore`, don't implement it

### 3. Distribution Module Rename (Not affecting us)
- `distributions` → `distr`

**Impact:** ✅ None - we don't use distributions

### 4. MSRV Bump
- Minimum Rust version: 1.63.0

**Impact:** ✅ None - we're already on Rust 1.90.0+

---

## Proposed Solution

### Step 1: Update Workspace Dependencies

Update `rust/Cargo.toml`:

```toml
[workspace.dependencies]
rand = "0.9"  # Was: 0.8
```

### Step 2: Verify Compatibility

The `OsRng` we use comes from `aes_gcm` crate, which provides it via re-export from `rand_core`. We need to verify that `aes-gcm` 0.10 is compatible with `rand_core` 0.9.

**Compatibility check:**
```bash
cd rust
cargo tree -p aes-gcm -p rand_core
```

### Step 3: Update Direct Dependency (if needed)

If `vm-auth-proxy` needs explicit rand version, update `vm-auth-proxy/Cargo.toml`:

```toml
[dependencies]
rand = { workspace = true }  # Already using workspace version
```

### Step 4: Code Changes

**Option A:** If `OsRng` from `aes_gcm` is compatible → **NO CODE CHANGES**

**Option B:** If incompatible, change import in `vm-auth-proxy/src/crypto.rs`:

```rust
// Before:
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use rand::RngCore;

// After:
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::{RngCore, rngs::OsRng};
```

### Step 5: Run Tests

```bash
cargo test --package vm-auth-proxy
cargo test --workspace
```

---

## Migration Plan

### Phase 1: Investigation (30 minutes)
1. Check `aes-gcm` compatibility with `rand_core` 0.9
2. Identify if any other transitive dependencies conflict
3. Create a test branch

### Phase 2: Implementation (1-2 hours)
1. Update workspace `Cargo.toml` with `rand = "0.9"`
2. Run `cargo update -p rand -p rand_core -p rand_chacha -p getrandom`
3. Fix any compilation errors (likely none or minimal)
4. Run full test suite
5. Test crypto operations specifically:
   ```bash
   cargo test --package vm-auth-proxy -- test_encryption_roundtrip
   cargo test --package vm-auth-proxy -- test_salt_generation
   cargo test --package vm-auth-proxy -- test_auth_token_generation
   ```

### Phase 3: Validation (30 minutes)
1. Manual testing of auth proxy
2. Verify encrypted secrets still decrypt correctly
3. Security review of any crypto changes

---

## Success Criteria

- ✅ All tests passing
- ✅ `cargo outdated` shows rand ecosystem at latest versions
- ✅ No clippy warnings introduced
- ✅ Existing encrypted secrets still decrypt
- ✅ New secrets encrypt/decrypt correctly

---

## Risks & Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| `aes-gcm` incompatible with `rand_core` 0.9 | Low | Medium | Use `rand::rngs::OsRng` directly instead |
| Breaking crypto compatibility | Very Low | High | Extensive testing of encrypt/decrypt |
| Transitive dependency conflicts | Medium | Low | Use `cargo tree` to identify conflicts |
| Security regression | Very Low | Critical | Review rand 0.9 changelog for security notes |

---

## Alternatives Considered

### 1. Stay on rand 0.8
**Pros:** No work, no risk
**Cons:** Technical debt, potential security issues, ecosystem divergence
**Decision:** Rejected - small effort for significant benefit

### 2. Remove rand dependency entirely
**Pros:** One less dependency
**Cons:** `OsRng` still comes transitively via `aes-gcm`
**Decision:** Not feasible - we need cryptographic RNG

### 3. Wait for rand 0.10
**Pros:** Skip this migration
**Cons:** Delays benefits, 0.10 may not come for years
**Decision:** Rejected - 0.9 is stable now

---

## Testing Plan

### Unit Tests (Automated)
```bash
# Crypto operations
cargo test --package vm-auth-proxy crypto

# All workspace tests
cargo test --workspace
```

### Integration Tests
```bash
# Start auth proxy and verify operations
cargo run --package vm-auth-proxy -- start
# Add a secret
vm auth add-secret TEST_KEY test_value
# Retrieve it
vm auth get-secret TEST_KEY
# Verify it matches
```

### Security Tests
1. Encrypt a secret with current version
2. Upgrade to rand 0.9
3. Verify old secrets still decrypt
4. Verify new secrets encrypt correctly
5. Compare entropy/randomness quality (optional)

---

## Documentation Updates

### CHANGELOG.md
```markdown
### Changed
- Updated `rand` crate from 0.8 to 0.9
  - No user-facing changes
  - Improved security and performance in cryptographic operations
```

### No User-Facing Changes
This is a purely internal upgrade with no API or behavioral changes.

---

## Estimated Effort

- **Investigation:** 30 minutes
- **Implementation:** 1-2 hours
- **Testing:** 30 minutes
- **Documentation:** 15 minutes
- **Total:** 2-3 hours

---

## Recommendation

**Proceed with upgrade** - Low risk, high value, minimal effort.

The upgrade is straightforward because:
1. We use minimal rand API surface area
2. The `RngCore` trait is stable
3. We can test crypto operations thoroughly
4. Small scope (one file)

Suggest implementing in next maintenance window.
