## Problem

Production code contains 124 `.unwrap()` calls, 11 `panic!()` calls, and 317 `.expect()` calls that should use proper error propagation. This creates crash risks in production and provides poor error messages to users.

## Solution(s)

1. **Replace unwraps:** Convert all 124 `.unwrap()` calls in production code to use the `?` operator for proper error propagation
2. **Remove panics:** Convert 11 `panic!()` calls in `schema.rs` and `yaml/formatter.rs` to return `Result<T, E>`
3. **Improve expects:** Add contextual messages to `.expect()` calls or replace with `?` where appropriate
4. **Create error macros:** Reduce duplication in 92 `VmError::Config` error creation patterns

## Checklists

- [ ] **Unwrap removal:**
    - [ ] Fix `vm-config/src/validator.rs` unwraps (lines 116, 155)
    - [ ] Audit and fix remaining 122 unwraps across codebase
    - [ ] Search for `.unwrap()` to verify all production instances removed
- [ ] **Panic removal:**
    - [ ] Convert `schema.rs` panics (lines 464, 477, 506) to Result returns
    - [ ] Convert `yaml/formatter.rs` panics (lines 226, 246) to Result returns
    - [ ] Update callers to handle new Result types
- [ ] **Expect improvements:**
    - [ ] Audit `.expect()` calls in vm-package-server and vm-auth-proxy
    - [ ] Add descriptive context to all `.expect()` messages
    - [ ] Replace with `?` operator where function already returns Result
- [ ] **Error handling utilities:**
    - [ ] Create `config_error!` macro for VmError::Config patterns
    - [ ] Create `map_config_err!` macro for common map_err patterns
    - [ ] Update codebase to use new macros
- [ ] **Verification:**
    - [ ] Run `cargo clippy` to catch remaining unwraps
    - [ ] Search codebase for `panic!`, `.unwrap()`, `.expect()` in src/ dirs
    - [ ] Run full test suite to verify error handling works

## Success Criteria

- Zero `.unwrap()` calls in production code (excluding test modules)
- Zero `panic!()` calls in production code (excluding invariant violations)
- All `.expect()` calls have descriptive context messages
- Error handling macros created and used in at least 50 locations
- All tests pass without panics or unwraps causing issues

## Benefits

- Eliminates crash risks from unwrap/panic in production
- Provides better error messages to users
- Makes error handling patterns consistent across codebase
- Reduces code duplication in error creation
- Improves debuggability with contextual error information
