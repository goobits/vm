# Testing Action Plan

This document outlines the structure of the integration tests for the `vm` crate and the plan for ongoing test improvements.

## Current Test Structure (`vm/tests/`)

The integration tests for the `vm` crate are organized by feature into the following directories to improve maintainability and discoverability.

```
vm/tests/
├── cli/
│   ├── config_commands.rs      # Config CLI tests
│   └── pkg_commands.rs         # Package CLI tests
├── common/
│   └── mod.rs                  # Shared test helpers
├── networking/
│   ├── port_forwarding.rs      # Port forwarding tests
│   └── ssh_refresh.rs          # SSH worktree refresh tests
├── services/
│   └── shared_services.rs      # Multi-VM service sharing
├── vm_ops/
│   ├── create_destroy_tests.rs # Core vm create/destroy lifecycle tests
│   └── ...                     # Other vm ops tests (lifecycle, features, etc.)
└── *.rs                        # Standalone or legacy test files, and module entrypoints
```

Each subdirectory (`cli`, `networking`, `services`) has a corresponding `mod.rs` or `*.rs` file at the root of `vm/tests/` that declares the test files within it as modules, allowing `cargo test` to discover them.

## Action Plan

1.  **Migrate Legacy Tests**:
    - [ ] Move `workflow_tests.rs` and `temp_workflow_tests.rs` into an appropriate subdirectory (e.g., `workflows/` or `integration/`).
    - [ ] Update module declarations after moving.

2.  **Improve Test Coverage**:
    - [ ] Add more tests for edge cases in `networking/port_forwarding.rs`.
    - [ ] Expand `cli/pkg_commands.rs` to cover more `vm pkg` subcommands.
    - [ ] Add tests for different provider configurations (e.g., Tart, Vagrant) where applicable.

3.  **Refactor `common/` Helpers**:
    - [ ] Organize the shared test helpers in `common/` into more specific modules (e.g., `common/fixtures.rs`, `common/assertions.rs`).

4.  **Documentation**:
    - [ ] Ensure all new test files have module-level documentation explaining their purpose.
    - [ ] Keep this document (`TESTING_ACTION_PLAN.md`) up-to-date with any future structural changes.
