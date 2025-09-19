# Error Handling Philosophy

This module provides a structured, user-friendly approach to error handling.

## Guiding Principles

1. **User-Friendly Messages:** All errors shown to the user should be clear, concise, and helpful. Use the `vm_error!`, `vm_warning!`, and `vm_error_hint!` macros.
2. **Centralized Logic:** Error creation logic should not be scattered throughout the application. It should be centralized into reusable functions within this module.
3. **Clean Application Code:** The main application logic should be kept clean of error formatting. Instead of building an error message manually, it should make a single call to an error function.

## Available Error Domains

- **`config`** - Configuration file parsing, validation, and missing configs
- **`network`** - Network connectivity, port conflicts, and communication failures
- **`provider`** - Provider-specific errors (Docker, Vagrant, Tart)
- **`package`** - Package management, installation, and validation errors
- **`temp`** - Temporary VM operations, state management, and mount operations
- **`installer`** - Binary building, installation, and project setup failures
- **`validation`** - Configuration validation and field checking errors

## How to Use

- **DO** create a new, descriptive function in the appropriate submodule (e.g., `config.rs`, `provider.rs`) for any common, user-facing error.
- **DO** use the `vm_error!` and related macros inside these functions to format the user message.
- **DO** return an `anyhow::Error` from these functions.
- **DO NOT** call `vm_error!` or `anyhow::anyhow!` with user-facing messages directly from the main application logic. Call a function from this module instead.

### Examples

**Good:**
```rust
// In application logic
return Err(errors::provider::docker_connection_failed());
return Err(errors::package::empty_script_name());
return Err(errors::validation::missing_required_field("provider"));
```

**Bad:**
```rust
// In application logic
vm_error!("Failed to connect to Docker");
vm_error_hint!("Try running docker info");
return Err(anyhow::anyhow!("Docker connection failed"));
```

## Migration Status

✅ **Completed Domains:**
- Configuration errors (`config.rs`)
- Network errors (`network.rs`)
- Provider errors (`provider.rs`)
- Package management errors (`package.rs`)
- Temporary VM errors (`temp.rs`)
- Installer errors (`installer.rs`)
- Validation errors (`validation.rs`)

🔄 **Migration Progress:**
- **vm-pkg**: ✅ Fully migrated (12 patterns)
- **vm-temp**: ✅ Key patterns migrated (1 pattern)
- **vm-config**: ✅ Validation patterns migrated (5 patterns)
- **vm-installer**: ✅ Build patterns migrated (4 patterns)
- **vm-provider**: ✅ Provider patterns migrated (existing)
- **vm-common**: ✅ All error modules with comprehensive tests

Total migrated: **22+ error patterns** from direct `vm_error!` + `anyhow::anyhow!` to centralized functions.