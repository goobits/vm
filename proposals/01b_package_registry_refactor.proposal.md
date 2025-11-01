## Problem

The files `rust/vm-package-server/src/npm.rs` and `rust/vm-package-server/src/pypi.rs` contain over 700 lines of code each, with significant structural and logical duplication for handling package management tasks.

## Solution(s)

Create a shared `PackageRegistry` trait to abstract the common functionality.

1.  Define a `PackageRegistry` trait with methods for `count_packages`, `list_all_packages`, `get_package_versions`, `download_package`, `publish_package`, etc.
2.  Create `NpmRegistry` and `PypiRegistry` structs that implement this trait.
3.  Move shared utility functions (e.g., file system interactions, pattern matching) into a common module (e.g., `package_utils.rs`) if they aren't already.

## Checklists

- [ ] Define a `PackageRegistry` trait in `rust/vm-package-server/src/lib.rs` or a new traits file.
- [ ] Refactor `npm.rs` to implement the `PackageRegistry` trait.
- [ ] Refactor `pypi.rs` to implement the `PackageRegistry` trait.
- [ ] Identify and move any duplicated helper functions into `package_utils.rs`.
- [ ] Update the Axum handlers to use the new trait-based structures.
- [ ] Ensure all changes compile successfully with `cargo check --all-targets`.
- [ ] Run the existing test suite with `cargo test --all-targets` to verify functionality.

## Success Criteria

- The amount of duplicated code between `npm.rs` and `pypi.rs` is reduced by at least 200 lines.
- A `PackageRegistry` trait exists and is implemented by both NPM and PyPI handlers.
- All existing package server tests pass.

## Benefits

- Reduces code duplication, making maintenance easier.
- Provides a clear interface for adding new package registry types in the future.
- Improves code organization and readability.
