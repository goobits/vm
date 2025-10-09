# Proposal: Clean Up `vm-package-server/src/lib.rs`

## Status
**Proposed** - Not yet implemented

## Problem Statement

The `vm-package-server/src/lib.rs` file contains **1,224 lines** when a library root file should be **~100 LOC** (module declarations + re-exports).

**Current issues**:
- Library root contains **300+ LOC of utility functions** (normalize_pypi_name, hash functions, validate_filename)
- Violates Rust best practices: lib.rs should be thin (module coordinator, not implementation)
- Utility functions scattered: some in lib.rs, some in validation_utils.rs
- Harder to find utilities: developers don't know whether to look in lib.rs or dedicated modules

### Current Metrics
- **Total LOC**: 1,224
- **Module declarations**: ~20 lines
- **Re-exports**: ~20 lines
- **Utility functions**: ~300 lines (should be in dedicated modules)
- **Test utils**: ~90 lines (acceptable under `#[cfg(test)]`)
- **Security tests**: ~800 lines (acceptable under `#[cfg(test)]`)

### Utility Functions in lib.rs (Lines to Move)

```
normalize_pypi_name()     - Lines 96-107   (PyPI name normalization)
sha256_hash()             - Lines 131-136  (SHA256 hashing)
sha1_hash()               - Lines 160-165  (SHA1 hashing for npm)
validate_filename()       - Lines 203-299  (Security validation)
```

**Total utility LOC to move**: ~200 lines

---

## Proposed Solution

Move utility functions to dedicated, domain-specific modules:

### Option A: Create New Modules (Recommended)

```
vm-package-server/src/
├── lib.rs               (~100 LOC) - Module declarations + re-exports only
├── pypi_utils.rs        (~20 LOC)  - PyPI-specific utilities (NEW)
├── hash_utils.rs        (~30 LOC)  - Hashing utilities (NEW)
├── validation_utils.rs  (400 LOC)  - Consolidated validation (EXPAND EXISTING)
└── ... (existing modules)
```

**Changes**:
1. Create `pypi_utils.rs` → move `normalize_pypi_name()`
2. Create `hash_utils.rs` → move `sha256_hash()`, `sha1_hash()`
3. Move `validate_filename()` to existing `validation_utils.rs`
4. Update lib.rs to re-export from new modules

### Option B: Consolidate into Existing Modules (Simpler)

```
vm-package-server/src/
├── lib.rs               (~100 LOC) - Module declarations + re-exports only
├── validation_utils.rs  (500 LOC)  - All validation + filename + hashing (EXPAND)
├── pypi.rs              (750 LOC)  - PyPI registry + normalize_pypi_name (ADD)
└── ... (existing modules)
```

**Changes**:
1. Move `normalize_pypi_name()` to `pypi.rs` (domain-specific)
2. Move `sha256_hash()`, `sha1_hash()`, `validate_filename()` to `validation_utils.rs`
3. Update lib.rs to re-export from existing modules

---

## Recommended Approach: **Option A**

**Rationale**:
- **Clearer organization**: Hash utils are separate from validation
- **Better discoverability**: `hash_utils.rs` vs "search through validation_utils.rs"
- **Future-proof**: Room for additional utility categories
- **Small modules**: 20-30 LOC each, easy to understand

---

## Detailed Module Breakdown (Option A)

### `lib.rs` (~100 LOC) **[After cleanup]**
**Responsibility**: Module coordinator - declarations + re-exports ONLY

**Structure**:
```rust
//! # Package Registry Server
//! (existing doc comment - keep)

// Module declarations (lines 31-52 - keep)
pub mod api;
pub mod auth;
pub mod cargo;
// ... etc

pub mod simple_config;

// NEW module declarations
pub mod pypi_utils;   // PyPI-specific utilities
pub mod hash_utils;   // Hashing utilities

// Re-export key types for convenience (lines 57-69 - keep)
pub use client_ops::{add_package, list_packages, remove_package, show_status};
pub use config::Config;
pub use error::{ApiErrorResponse, AppError, AppResult, ErrorCode};
pub use server::{run_server, run_server_background, run_server_with_shutdown};
pub use state::{AppState, SuccessResponse};
pub use upstream::{UpstreamClient, UpstreamConfig};
pub use validation::{...};

// NEW: Re-export utility functions from dedicated modules
pub use hash_utils::{sha1_hash, sha256_hash};
pub use pypi_utils::normalize_pypi_name;
pub use validation_utils::validate_filename;  // Moved from lib.rs

// Test utilities (lines 302-388 - keep under #[cfg(test)])
#[cfg(test)]
pub mod test_utils { ... }

// Security tests (lines 390-1224 - keep under #[cfg(test)])
#[cfg(test)]
mod security_tests { ... }
```

**After**: ~100 LOC (excluding tests which are acceptable)

---

### `pypi_utils.rs` (~20 LOC) **[NEW]**
**Responsibility**: PyPI-specific utility functions

**Lines moved from lib.rs**: 73-107

**Structure**:
```rust
//! PyPI-specific utility functions

use regex::Regex;
use std::sync::OnceLock;

/// Normalize PyPI package name according to PEP 503.
///
/// This function normalizes package names by converting them to lowercase and
/// replacing runs of `[-_.]+` with a single `-` character.
///
/// # Examples
///
/// ```
/// # use vm_package_server::pypi_utils::normalize_pypi_name;
/// assert_eq!(normalize_pypi_name("Django-REST-framework"), "django-rest-framework");
/// assert_eq!(normalize_pypi_name("some_package"), "some-package");
/// ```
pub fn normalize_pypi_name(name: &str) -> String {
    use regex::Regex;
    use std::sync::OnceLock;

    static PYPI_NAME_REGEX: OnceLock<Regex> = OnceLock::new();
    let re = PYPI_NAME_REGEX.get_or_init(|| {
        Regex::new(r"[-_.]+").unwrap_or_else(|e| {
            panic!("Failed to compile PyPI name normalization regex: {}", e)
        })
    });
    re.replace_all(&name.to_lowercase(), "-").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_pypi_name() {
        assert_eq!(normalize_pypi_name("Django-REST-framework"), "django-rest-framework");
        assert_eq!(normalize_pypi_name("some_package"), "some-package");
        assert_eq!(normalize_pypi_name("package.name"), "package-name");
    }
}
```

**Why separate from pypi.rs**: Utility function, not registry implementation logic

---

### `hash_utils.rs` (~30 LOC) **[NEW]**
**Responsibility**: Cryptographic hashing utilities

**Lines moved from lib.rs**: 109-165

**Structure**:
```rust
//! Cryptographic hashing utilities for package integrity verification

/// Calculate SHA256 hash of data.
///
/// Computes the SHA256 hash of the provided byte data and returns it as a
/// lowercase hexadecimal string. This is commonly used for package integrity
/// verification and checksum generation.
///
/// # Examples
///
/// ```
/// # use vm_package_server::hash_utils::sha256_hash;
/// let data = b"hello world";
/// let hash = sha256_hash(data);
/// assert_eq!(hash.len(), 64); // SHA256 produces 64 hex characters
/// ```
pub fn sha256_hash(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Calculate SHA1 hash of data (for npm).
///
/// Computes the SHA1 hash of the provided byte data and returns it as a
/// lowercase hexadecimal string. This is specifically used for npm package
/// integrity checks, as npm uses SHA1 hashes in its metadata.
///
/// # Examples
///
/// ```
/// # use vm_package_server::hash_utils::sha1_hash;
/// let data = b"hello world";
/// let hash = sha1_hash(data);
/// assert_eq!(hash.len(), 40); // SHA1 produces 40 hex characters
/// ```
pub fn sha1_hash(data: &[u8]) -> String {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_hash() {
        let data = b"hello world";
        let hash = sha256_hash(data);
        assert_eq!(hash.len(), 64);
        // Known SHA256 hash for "hello world"
        assert_eq!(hash, "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9");
    }

    #[test]
    fn test_sha1_hash() {
        let data = b"hello world";
        let hash = sha1_hash(data);
        assert_eq!(hash.len(), 40);
        // Known SHA1 hash for "hello world"
        assert_eq!(hash, "2aae6c35c94fcfb415dbe95f408b9ce91ee846ed");
    }
}
```

**Why separate module**: Hashing is orthogonal to validation logic

---

### `validation_utils.rs` (500 LOC) **[Expanded]**
**Responsibility**: File validation and security utilities (already exists)

**Lines added from lib.rs**: 167-299 (validate_filename)

**Current state**: Already has FileStreamValidator, DockerValidator (373 LOC)

**After adding validate_filename**: ~500 LOC (still reasonable)

**Changes**:
```rust
// Add to existing validation_utils.rs

/// Validates a filename to prevent path traversal attacks and other security issues.
///
/// (move entire function from lib.rs lines 203-299)
pub fn validate_filename(filename: &str) -> Result<(), AppError> {
    // ... (existing implementation from lib.rs)
}
```

**Note**: File already has security-focused utilities, so `validate_filename` is a natural fit

---

## API Compatibility

### External API: **100% Backward Compatible**

All existing imports will continue to work:

```rust
// Before and After - SAME IMPORTS
use vm_package_server::{
    normalize_pypi_name,  // Now from pypi_utils (via re-export)
    sha256_hash,          // Now from hash_utils (via re-export)
    sha1_hash,            // Now from hash_utils (via re-export)
    validate_filename,    // Now from validation_utils (via re-export)
};

// All function signatures unchanged
let normalized = normalize_pypi_name("Django-REST");
let hash = sha256_hash(b"data");
validate_filename("package.whl")?;
```

**How**: lib.rs uses `pub use` to re-export functions from new modules

---

## Implementation Plan

### Single PR Approach (Recommended)

**Why single PR**: This is a simple refactor (200 LOC move), not like lifecycle (2,559 LOC)

**Changes**:
1. Create `src/pypi_utils.rs` (move `normalize_pypi_name` from lib.rs)
2. Create `src/hash_utils.rs` (move `sha256_hash`, `sha1_hash` from lib.rs)
3. Move `validate_filename` from lib.rs to existing `src/validation_utils.rs`
4. Update `lib.rs`:
   - Add module declarations: `pub mod pypi_utils;`, `pub mod hash_utils;`
   - Add re-exports: `pub use pypi_utils::normalize_pypi_name;` etc.
   - Remove moved function implementations
5. Run tests: `cargo test --package vm-package-server`
6. Update CHANGELOG

**Estimated time**: 30 minutes

**Reviewability**: ~200 LOC mechanical move, clear boundaries

---

## Benefits

### Maintainability
- ✅ lib.rs becomes thin coordinator (~100 LOC vs 1,224 LOC)
- ✅ Follows Rust best practices (lib.rs for coordination, not implementation)
- ✅ Clear utility organization (hash_utils, pypi_utils, validation_utils)

### Discoverability
- ✅ Developers know where to find utilities (`hash_utils.rs` vs "somewhere in lib.rs")
- ✅ Module names indicate purpose
- ✅ Easier to navigate project structure

### Testability
- ✅ Can add module-specific tests
- ✅ Test utilities separate from implementation utilities
- ✅ Better test organization

### Documentation
- ✅ Module-level docs can explain utility category
- ✅ Clearer doc structure in generated rustdoc
- ✅ Easier to link to utility docs

---

## Non-Goals

This refactoring **does not**:
- ❌ Change functionality or fix bugs
- ❌ Modify public API surface (import paths via re-export)
- ❌ Add new features
- ❌ Change existing module implementations (pypi.rs, npm.rs, etc. unchanged)
- ❌ Modify test code behavior

---

## Risks & Mitigation

### Risk: Breaking external imports
**Mitigation**:
- Use `pub use` re-exports in lib.rs
- External code still imports from `vm_package_server::{normalize_pypi_name, ...}`
- No changes to import paths required

### Risk: Missed function usages
**Mitigation**:
- Compiler will catch all usages (Rust's strong type system)
- Run full test suite to verify
- Functions are public, so compiler ensures consistency

### Risk: Test failures
**Mitigation**:
- Keep existing tests in lib.rs (security_tests mod)
- Add module-specific tests in new files
- Run `cargo test --package vm-package-server` before/after

---

## Success Criteria

1. ✅ lib.rs reduced to ~100 LOC (excluding tests)
2. ✅ All utility functions in dedicated modules
3. ✅ All existing tests pass without modification
4. ✅ No changes to external API (imports unchanged)
5. ✅ `cargo clippy` passes with no new warnings
6. ✅ Documentation builds successfully

---

## Alternative Considered: Option B (Consolidate into Existing Modules)

**Why rejected**:
- validation_utils.rs would grow to 500+ LOC (still large)
- Hashing logic mixed with validation logic (less clear)
- PyPI normalization in pypi.rs mixes registry impl with utilities
- Harder to find utilities ("is it in pypi.rs or validation_utils.rs?")

**When to reconsider**: If team prefers fewer modules over clearer organization

---

## Comparison with Other Refactors

| Aspect | Lifecycle Refactor | VM Ops Refactor | Lib Cleanup |
|--------|-------------------|-----------------|-------------|
| **LOC to move** | 2,559 | 1,507 | ~200 |
| **Modules created** | 9 | 7 | 2 |
| **Complexity** | High | Medium | **Low** |
| **PRs needed** | 7 | 8 | **1** |
| **Estimated time** | 11 hours | 8 hours | **30 min** |

**This is the easiest of the three refactors**: Just move 3-4 functions to new modules

---

## References

- **Current file**: `rust/vm-package-server/src/lib.rs` (1,224 LOC)
- **Existing module**: `rust/vm-package-server/src/validation_utils.rs` (373 LOC)
- **Related files**: `pypi.rs` (uses normalize_pypi_name), `npm.rs` (uses sha1_hash)

---

## Changelog Entry

```markdown
### Changed
- **Cleaned up package server library root**: Moved utility functions from `lib.rs` to dedicated modules
  - Created `pypi_utils.rs` for PyPI name normalization
  - Created `hash_utils.rs` for SHA256/SHA1 hashing
  - Moved `validate_filename()` to existing `validation_utils.rs`
  - lib.rs now ~100 LOC (module declarations + re-exports)
- **No API changes**: All imports unchanged via public re-exports
- **Improved organization**: Utility functions now in domain-specific modules
```

---

## Approval

- [ ] Approved by: _______________
- [ ] Date: _______________
- [ ] Implementation branch: `refactor/package-server-lib-cleanup`
