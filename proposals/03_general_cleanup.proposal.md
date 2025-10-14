## Problem

Several parts of the codebase suffer from code quality issues due to organic growth, including a monolithic message file, unnecessary allocations from `.clone()`, and deeply nested logic.

## Solution(s)

1.  **Messages:** Split `rust/vm-messages/src/messages.rs` by domain. Create a hierarchical module structure (e.g., `messages::vm::create`, `messages::plugin::list`) and use a builder or macro to access messages, reducing the size of the central `Messages` struct.
2.  **Nesting:** Refactor `rust/vm-provider/src/docker/compose.rs` by extracting logic into smaller, well-named functions and using guard clauses to reduce deep nesting.
3.  **Clones:** Analyze the 350+ `.clone()` calls. Replace unnecessary clones on `Copy` types with direct copies, and use references (`&`) or move semantics where ownership transfer is appropriate to reduce heap allocations.

## Checklists

- [ ] **Messages:**
    - [ ] Create a directory `rust/vm-messages/src/messages/`.
    - [ ] Move related message strings from `messages.rs` into domain-specific submodules (e.g., `vm.rs`, `plugin.rs`).
    - [ ] Refactor the `MESSAGES` constant to pull from the new modules.
- [ ] **Nesting:**
    - [ ] Identify functions in `docker/compose.rs` with high cyclomatic complexity.
    - [ ] Extract conditional blocks into separate private functions.
    - [ ] Replace nested `if` statements with guard clauses where possible.
- [ ] **Clones:**
    - [ ] Search for all `.clone()` calls in the codebase.
    - [ ] For each call, determine if it is necessary.
    - [ ] Replace unnecessary clones with references or moves.
- [ ] **Verification:**
    - [ ] Ensure all changes compile with `cargo check --all-targets`.
    - [ ] Run all tests with `cargo test --all-targets` to confirm no regressions.

## Success Criteria

- The `messages.rs` file is broken into at least 5 smaller, domain-focused modules.
- The maximum nesting level in `docker/compose.rs` functions is reduced to 3.
- The number of `.clone()` calls is reduced by at least 25%.
- All existing tests pass.

## Benefits

- Improves overall code readability and maintainability.
- Reduces cognitive load when working with messages and Docker Compose logic.
- Potentially improves performance by reducing unnecessary memory allocations.
