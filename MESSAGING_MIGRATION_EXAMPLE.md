# VM Messages System - Migration Example

This document demonstrates how to migrate existing message strings to use the new centralized `vm-messages` system.

## Before: Current Approach

```rust
// Current implementation in vm-temp/src/temp_ops.rs
println!(
    "✅ Temporary VM created with {} mount(s)",
    temp_state.mount_count()
);

println!("🔗 Connecting to temporary VM...");
println!("🗑️ Auto-destroying temporary VM...");
println!("💡 Use 'vm temp ssh' to connect");
println!("   Use 'vm temp destroy' when done");
```

## After: New Centralized System

### Step 1: Add messages to the central registry

```rust
// In vm-messages/src/messages.rs
pub struct Messages {
    // ... existing messages

    // Temp VM specific messages
    pub temp_vm_created_with_mounts: &'static str,
    pub temp_vm_connecting: &'static str,
    pub temp_vm_auto_destroying: &'static str,
    pub temp_vm_usage_hint: &'static str,
}

pub const MESSAGES: Messages = Messages {
    // ... existing messages

    temp_vm_created_with_mounts: "✅ Temporary VM created with {count} mount(s)",
    temp_vm_connecting: "🔗 Connecting to temporary VM...",
    temp_vm_auto_destroying: "🗑️ Auto-destroying temporary VM...",
    temp_vm_usage_hint: "💡 Use 'vm temp ssh' to connect\n   Use 'vm temp destroy' when done",
};
```

### Step 2: Update the implementation

```rust
// Updated implementation using the new system
use vm_common::messages::{msg, messages::MESSAGES};

// Replace this:
println!(
    "✅ Temporary VM created with {} mount(s)",
    temp_state.mount_count()
);

// With this:
vm_println!("{}", msg!(MESSAGES.temp_vm_created_with_mounts, count = temp_state.mount_count()));

// Replace this:
println!("🔗 Connecting to temporary VM...");

// With this:
vm_println!("{}", MESSAGES.temp_vm_connecting);

// Replace this:
println!("🗑️ Auto-destroying temporary VM...");

// With this:
vm_println!("{}", MESSAGES.temp_vm_auto_destroying);

// Replace this:
println!("💡 Use 'vm temp ssh' to connect");
println!("   Use 'vm temp destroy' when done");

// With this:
vm_println!("{}", MESSAGES.temp_vm_usage_hint);
```

### Step 3: For operation patterns, use semantic categories

```rust
// For VM operations, we can use the semantic categories
use vm_common::messages::categories::VM_OPS;

// Before:
println!("🚀 Creating temporary VM...");
// ... operation happens ...
if success {
    println!("✅ Created successfully");
} else {
    eprintln!("❌ Failed to create VM: {}", error);
}

// After:
vm_operation!(start create, name = "temporary VM");
// ... operation happens ...
if success {
    vm_operation!(success create);
} else {
    vm_operation!(failed create, name = "temporary VM", error = error);
}
```

## Benefits of the New System

1. **Centralization**: All user-facing messages are in one place
2. **Consistency**: Unified emoji usage and formatting
3. **Maintainability**: Easy to update messages without hunting through code
4. **Template Support**: Dynamic values with clear variable names
5. **Type Safety**: Compile-time checking of template variables
6. **Discoverability**: Easy to see all available messages

## Migration Path

1. **Gradual**: Messages can be migrated incrementally
2. **Safe**: Old macros (vm_println, vm_error, etc.) still work
3. **Testable**: Each migration can be tested independently
4. **Reviewable**: Changes are focused and easy to review

## Example Template Usage

```rust
// Simple message
vm_println!("{}", MESSAGES.docker_is_running);

// Message with variables
vm_println!("{}", msg!(MESSAGES.vm_is_running, name = vm_name));

// Multiple variables
vm_println!("{}", msg!(MESSAGES.config_set_success,
    field = "memory",
    value = "4096",
    path = "/workspace/.vm/config.yml"
));
```

## Semantic Operations

```rust
// VM lifecycle operations
vm_operation!(start create, name = "myvm");
vm_operation!(success create);
vm_operation!(failed create, name = "myvm", error = "Docker not running");

// Common suggestions
vm_suggest!(docker_check);
vm_suggest!(vm_create);
vm_suggest!(custom "Try: vm restart");
```

This new system provides a robust foundation for consistent, maintainable messaging across the entire VM tool ecosystem.