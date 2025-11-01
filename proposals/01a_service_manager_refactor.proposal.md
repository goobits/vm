## Problem

`rust/vm/src/service_manager.rs` is a "God object" managing the lifecycle of at least 7 distinct services. This violates the single-responsibility principle, making the code difficult to maintain, test, and extend.

## Solution(s)

Refactor `ServiceManager` using a trait-based architecture.

1.  Define a generic `ManagedService` trait with methods like `start()`, `stop()`, `health_check()`, and `name()`.
2.  Implement this trait for each service (PostgreSQL, Redis, Auth Proxy, etc.) in its own module.
3.  Modify `ServiceManager` to manage a collection of `Box<dyn ManagedService>` objects, delegating lifecycle operations to the trait methods instead of using a large `match` statement.

## Checklists

- [ ] Define a `ManagedService` trait in a new file (e.g., `rust/vm/src/services/traits.rs`).
- [ ] Create a separate module for each service (e.g., `rust/vm/src/services/postgres.rs`, `rust/vm/src/services/redis.rs`, etc.).
- [ ] Move the logic for each service from `service_manager.rs` into its respective module and implement the `ManagedService` trait.
- [ ] Update `ServiceManager` to hold a `HashMap<String, Box<dyn ManagedService>>`.
- [ ] Remove the large `match` statements in `start_service` and `stop_service`, replacing them with dynamic dispatch calls on the trait objects.
- [ ] Ensure all changes compile successfully with `cargo check --all-targets`.
- [ ] Run the existing test suite with `cargo test --all-targets` to verify functionality.

## Success Criteria

- The `service_manager.rs` file is significantly reduced in size and complexity.
- The large `match` statements for service dispatch are eliminated.
- Each service's lifecycle logic is encapsulated within its own module.
- All existing service-related tests pass.

## Benefits

- Improved maintainability and readability.
- Easier to add, remove, or modify services without changing `ServiceManager`.
- Better adherence to SOLID principles.
