# PROPOSAL_08_FIX_CONFIG_CLI_TESTS_FOR_TRACING.md

## Problem Statement

The `rust/vm/tests/config_cli_tests.rs` test suite is failing after the migration from `println!`/`eprintln!` to the `tracing` framework. The tests were written with assertions that expected exact string matches against `stdout`/`stderr`, but the tracing output now includes additional metadata (timestamps, log levels, target names) that breaks these assertions.

## Root Cause

The `tracing` framework outputs structured logs with formatting like:
```
2025-01-09T12:34:56.789Z  INFO vm_config: Configuration updated successfully
```

But the tests were checking for simple strings like:
```rust
assert_eq!(stderr, "Configuration updated successfully\n");
```

This mismatch causes all assertions to fail, even though the underlying functionality works correctly.

## Goals

1. **Fix the failing tests** in `config_cli_tests.rs` to work with tracing output
2. **Establish patterns** for testing CLI output that are resilient to logging format changes
3. **Document best practices** for future test development
4. **Maintain test coverage** without reducing assertion quality

## Proposed Solution

### Step 1: Analyze Current Test Failures

**Action:** Run the failing tests and capture the actual output

```bash
cd /workspace/rust
cargo test --package vm --test config_cli_tests -- --nocapture 2>&1 | tee test_output.log
```

**Expected Findings:**
- Identify which specific assertions are failing
- Capture the actual stderr/stdout content being produced
- Understand the exact format of tracing output in the test context

### Step 2: Understand Tracing Test Behavior

**Action:** Review how tracing is initialized in the test environment

**Key Questions:**
1. Is `init_tracing()` called before tests run?
2. Does tracing output go to stderr or stdout in tests?
3. What format is being used (plain text, JSON, etc.)?
4. Are there any test-specific tracing configurations?

**Files to Check:**
- `rust/vm/tests/config_cli_tests.rs` - Test setup
- `rust/vm/src/main.rs` - Tracing initialization
- `rust/vm-core/src/macros.rs` - Macro definitions

### Step 3: Create Helper Functions for Robust Assertions

**Action:** Add test helper functions at the top of `config_cli_tests.rs`

```rust
/// Helper to check if stderr contains a message, ignoring tracing metadata
fn stderr_contains_message(stderr: &str, expected: &str) -> bool {
    stderr.lines().any(|line| line.contains(expected))
}

/// Helper to assert stderr contains expected content
fn assert_stderr_contains(stderr: &str, expected: &str) {
    assert!(
        stderr_contains_message(stderr, expected),
        "Expected stderr to contain '{}'\nActual stderr:\n{}",
        expected,
        stderr
    );
}

/// Helper to check for error messages (case-insensitive)
fn stderr_contains_error(stderr: &str) -> bool {
    stderr.to_lowercase().contains("error")
}

/// Helper to extract just the message content from a tracing log line
/// Removes timestamp, level, and target, leaving just the message
fn extract_message(log_line: &str) -> &str {
    // Tracing format: "YYYY-MM-DDTHH:MM:SS.sssZ LEVEL target: message"
    // We want to extract just "message"

    // Strategy 1: Split on the last ": " (colon-space)
    if let Some(pos) = log_line.rfind(": ") {
        return &log_line[pos + 2..];
    }

    // Strategy 2: If no ": " found, check for level keywords and extract after them
    for level in &["ERROR", "WARN", "INFO", "DEBUG", "TRACE"] {
        if let Some(pos) = log_line.find(level) {
            let after_level = &log_line[pos + level.len()..];
            if let Some(msg_start) = after_level.find(": ") {
                return after_level[msg_start + 2..].trim();
            }
        }
    }

    // Fallback: return the whole line trimmed
    log_line.trim()
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn test_extract_message() {
        let log = "2025-01-09T12:34:56.789Z  INFO vm_config: Configuration updated successfully";
        assert_eq!(extract_message(log), "Configuration updated successfully");

        let log = "ERROR vm: Invalid configuration file";
        assert_eq!(extract_message(log), "Invalid configuration file");
    }
}
```

### Step 4: Update Test Assertions Pattern-by-Pattern

**Action:** Systematically update each test case using one of these patterns:

#### Pattern A: Success Message Verification
**Use when:** Testing successful command execution with informational output

**Before:**
```rust
let output = run_config_command(&["set", "worktrees.enabled", "true"])?;
assert_eq!(output.stderr, "Configuration updated successfully\n");
```

**After:**
```rust
let output = run_config_command(&["set", "worktrees.enabled", "true"])?;
assert!(output.status.success(), "Command should succeed");
assert_stderr_contains(&output.stderr, "Configuration updated");
assert_stderr_contains(&output.stderr, "worktrees.enabled");

// Most importantly: verify the actual behavior
let config = load_global_config()?;
assert_eq!(config.worktrees.enabled, true);
```

#### Pattern B: Error Message Verification
**Use when:** Testing error conditions and validation

**Before:**
```rust
let result = run_config_command(&["set", "invalid.key", "value"]);
assert!(result.is_err());
assert_eq!(stderr, "Error: Invalid configuration key\n");
```

**After:**
```rust
let output = run_config_command(&["set", "invalid.key", "value"])?;
assert!(!output.status.success(), "Command should fail");
assert!(stderr_contains_error(&output.stderr), "Should contain error message");
assert_stderr_contains(&output.stderr, "Invalid configuration key");

// Verify behavior: config should be unchanged
let config = load_global_config()?;
assert_eq!(config.get("invalid.key"), None);
```

#### Pattern C: Multi-line Output Verification
**Use when:** Testing list commands or verbose output

**Before:**
```rust
let output = run_config_command(&["get"])?;
assert_eq!(output.stdout, "worktrees.enabled = true\nworktrees.base_path = /tmp/worktrees\n");
```

**After:**
```rust
let output = run_config_command(&["get"])?;
assert!(output.status.success(), "Command should succeed");

// Check each expected line/value
assert_stderr_contains(&output.stderr, "worktrees.enabled");
assert_stderr_contains(&output.stderr, "true");
assert_stderr_contains(&output.stderr, "worktrees.base_path");
assert_stderr_contains(&output.stderr, "/tmp/worktrees");

// Or use structured parsing if output is meant to be machine-readable
// (consider if this command should output to stdout instead)
```

#### Pattern D: Silent Success (No Output Expected)
**Use when:** Commands that should succeed silently

**Before:**
```rust
let output = run_config_command(&["set", "provider.default", "docker"])?;
assert_eq!(output.stderr, "");
assert_eq!(output.stdout, "");
```

**After:**
```rust
let output = run_config_command(&["set", "provider.default", "docker"])?;
assert!(output.status.success(), "Command should succeed");

// Either expect no output or accept informational messages
// If we want silent success, the command itself should use debug! level
// For now, accept that there may be INFO messages
if !output.stderr.is_empty() {
    // If there is output, it should be informational, not an error
    assert!(!stderr_contains_error(&output.stderr),
        "Should not contain errors: {}", output.stderr);
}

// Verify behavior
let config = load_global_config()?;
assert_eq!(config.provider.default, "docker");
```

### Step 5: Decide on Output Streams for Different Message Types

**Action:** Establish conventions for where different messages should go

**Principle:**
- **stdout** = Structured data meant for piping/parsing (JSON, lists, values)
- **stderr** = Human-readable status messages, errors, warnings (tracing output)

**Decision Points:**

1. **Config `get` command** - Should output go to stdout or stderr?
   - **Recommendation:** stdout for the values, stderr for status messages
   - **Rationale:** Allows `vm config get worktrees.enabled | some_script`

2. **Config `set` command** - Should success messages go to stderr?
   - **Recommendation:** INFO level to stderr (via tracing), or silent success
   - **Rationale:** User feedback is helpful, but shouldn't pollute stdout

3. **Config `list` command** - Should output go to stdout or stderr?
   - **Recommendation:** stdout for the list, stderr for status
   - **Rationale:** Enables scripting and piping

**Action Items:**
- Review each CLI command in `vm-config/src/cli/commands/*.rs`
- Ensure data output uses `println!` to stdout
- Ensure status messages use `info!()`, `warn!()`, `error!()` to stderr
- Update tests accordingly

### Step 6: Handle Tracing Initialization in Tests

**Action:** Ensure tracing is properly initialized (or not) in test environment

**Option A: Disable Tracing in Tests (Simpler)**
```rust
// At the top of config_cli_tests.rs
fn setup_test_env() -> TempDir {
    // Disable tracing for cleaner test output
    std::env::set_var("RUST_LOG", "off");

    let temp = TempDir::new().unwrap();
    // ... rest of setup
    temp
}
```

**Option B: Initialize Test-Specific Tracing (More Realistic)**
```rust
use tracing_subscriber::fmt::format::FmtSpan;

fn setup_test_env() -> TempDir {
    // Initialize tracing once for all tests
    use std::sync::Once;
    static INIT: Once = Once::new();

    INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_env_filter("info")
            .with_test_writer() // Important: use test-specific writer
            .with_target(false) // Don't show target names
            .with_level(true)   // Show level
            .without_time()     // Don't show timestamps (cleaner for tests)
            .init();
    });

    let temp = TempDir::new().unwrap();
    // ... rest of setup
    temp
}
```

**Recommendation:** Use Option B (test-specific tracing) with `without_time()` and `with_target(false)` to make output more predictable.

### Step 7: Update Specific Failing Tests

**Action:** Go through each failing test in `config_cli_tests.rs` and apply the patterns

**Priority Order:**
1. `test_config_set_*` - Core functionality, highest impact
2. `test_config_get_*` - Data retrieval tests
3. `test_config_list` - List functionality
4. `test_config_validation_*` - Error handling tests
5. Any remaining tests

**Per-Test Checklist:**
- [ ] Identify what behavior is being tested
- [ ] Replace exact string assertions with `contains()` checks
- [ ] Add behavior verification (check config file, not just output)
- [ ] Use helper functions for common patterns
- [ ] Run the single test: `cargo test --package vm --test config_cli_tests test_name`
- [ ] Verify it passes

### Step 8: Add Regression Tests for Logging

**Action:** Create tests that explicitly verify logging behavior

```rust
#[test]
fn test_tracing_output_format() {
    // Ensure we understand what format tracing is producing
    let output = run_config_command(&["set", "test.key", "value"])?;

    // Should have some output on stderr
    assert!(!output.stderr.is_empty(), "Expected tracing output on stderr");

    // Output should contain the key message
    assert_stderr_contains(&output.stderr, "test.key");

    // Output should indicate success (no ERROR level)
    assert!(!stderr_contains_error(&output.stderr));
}

#[test]
fn test_error_logging_format() {
    let output = run_config_command(&["set", "invalid", "value"])?;

    // Should fail
    assert!(!output.status.success());

    // Should have error in output
    assert!(stderr_contains_error(&output.stderr));
    assert_stderr_contains(&output.stderr, "Invalid");
}
```

### Step 9: Document Testing Best Practices

**Action:** Add a section to `CLAUDE.md` or create `TESTING_GUIDE.md`

```markdown
## Testing CLI Output with Tracing

### Principles

1. **Test behavior, not output format** - Verify config changes, not log formatting
2. **Use flexible assertions** - `contains()` instead of exact matches
3. **Separate data from status** - stdout for data, stderr for status
4. **Use helper functions** - Centralize assertion logic

### Example Test Pattern

```rust
#[test]
fn test_config_command() -> Result<()> {
    let temp = setup_test_env();

    // Run command
    let output = run_config_command(&["set", "key", "value"])?;

    // Check exit status
    assert!(output.status.success());

    // Check output contains key messages
    assert_stderr_contains(&output.stderr, "key");

    // Most important: verify actual behavior
    let config = load_config()?;
    assert_eq!(config.key, "value");

    Ok(())
}
```

### Common Pitfalls

- ❌ Don't use `assert_eq!` on full output strings
- ❌ Don't assume specific log formatting
- ❌ Don't test timestamps or log levels directly
- ✅ Do test that key messages appear
- ✅ Do test actual behavior (file changes, config updates)
- ✅ Do use helper functions for common patterns
```

### Step 10: Run Full Test Suite and Verify

**Action:** Ensure all tests pass

```bash
cd /workspace/rust

# Run just the config CLI tests
cargo test --package vm --test config_cli_tests -- --nocapture

# Run all tests
cargo test --workspace

# Run quality gates
cd /workspace
make quality-gates
```

**Success Criteria:**
- All tests in `config_cli_tests.rs` pass
- No regressions in other test files
- Test output is clear and helpful
- Tests are maintainable and resilient to logging format changes

## Implementation Checklist

- [ ] **Step 1:** Capture current test failures and analyze output format
- [ ] **Step 2:** Understand how tracing is initialized in tests
- [ ] **Step 3:** Add helper functions for assertions
- [ ] **Step 4:** Choose assertion patterns for different test types
- [ ] **Step 5:** Audit CLI commands for stdout vs stderr usage
- [ ] **Step 6:** Set up test-specific tracing configuration
- [ ] **Step 7:** Update each failing test systematically
- [ ] **Step 8:** Add regression tests for logging behavior
- [ ] **Step 9:** Document best practices
- [ ] **Step 10:** Verify all tests pass

## Non-Goals

- Changing the tracing format itself (that's already standardized)
- Removing test coverage (we're fixing tests, not removing them)
- Making tests more brittle (we're making them more flexible)
- Testing tracing internals (we test our application behavior)

## Success Metrics

1. **All tests pass** - `cargo test --workspace` exits with success
2. **Tests are maintainable** - Future log format changes won't break tests
3. **Tests verify behavior** - Not just output format
4. **Clear patterns** - Future developers can easily write similar tests

## Future Considerations

1. **Structured Output Option** - Consider adding `--json` flag to commands for machine-readable output
2. **Test Utilities Crate** - If more test files need similar helpers, extract to `vm-test-utils`
3. **Snapshot Testing** - For complex output, consider using `insta` crate for snapshot tests
4. **Integration Test Separation** - Consider splitting CLI tests from unit tests

---

## Summary for Future Developers

If you encounter failing tests after logging changes:

1. **Don't panic** - The functionality likely works, tests just need updating
2. **Focus on behavior** - Verify what changed (files, configs), not log format
3. **Use `contains()`** - Not exact string matches
4. **Add helpers** - Don't repeat assertion logic
5. **Test incrementally** - Fix one test at a time
6. **Ask for help** - Show the actual vs expected output when stuck

The key insight: **Logging is for humans, tests should verify behavior for machines.**
