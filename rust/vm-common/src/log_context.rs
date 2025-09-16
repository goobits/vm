use serde_json::{Map, Value};
use std::cell::RefCell;

// Thread-local storage for logging context
thread_local! {
    static LOG_CONTEXT: RefCell<Vec<Map<String, Value>>> = RefCell::new(vec![Map::new()]);
}

/// Get the current logging context as a merged map
pub fn current_context() -> Map<String, Value> {
    LOG_CONTEXT.with(|stack| {
        let stack = stack.borrow();
        let mut merged = Map::new();

        // Merge all context layers from bottom to top
        for layer in stack.iter() {
            for (key, value) in layer {
                merged.insert(key.clone(), value.clone());
            }
        }

        merged
    })
}

/// Push a new context layer onto the stack
pub fn push_context(context: Map<String, Value>) {
    LOG_CONTEXT.with(|stack| {
        stack.borrow_mut().push(context);
    });
}

/// Pop the top context layer from the stack
pub fn pop_context() {
    LOG_CONTEXT.with(|stack| {
        let mut stack = stack.borrow_mut();
        if stack.len() > 1 {
            stack.pop();
        }
    });
}

/// Add key-value pairs to the current top context layer
pub fn add_to_current_context<K, V>(key: K, value: V)
where
    K: Into<String>,
    V: Into<Value>,
{
    LOG_CONTEXT.with(|stack| {
        let mut stack = stack.borrow_mut();
        if let Some(current) = stack.last_mut() {
            current.insert(key.into(), value.into());
        }
    });
}

/// RAII context guard that automatically pops context when dropped
pub struct ContextGuard;

impl Drop for ContextGuard {
    fn drop(&mut self) {
        pop_context();
    }
}

/// Create a scoped context that will be automatically cleaned up
///
/// Usage:
/// ```
/// use vm_common::scoped_context;
/// let _guard = scoped_context! {
///     "operation" => "create",
///     "provider" => "docker"
/// };
/// // Context is automatically popped when _guard goes out of scope
/// ```
pub fn scoped_context(context: Map<String, Value>) -> ContextGuard {
    push_context(context);
    ContextGuard
}

/// Convenience macro for creating scoped context
///
/// Usage:
/// ```
/// use vm_common::scoped_context;
/// let _guard = scoped_context! {
///     "operation" => "create",
///     "component" => "docker",
///     "request_id" => "abc123"
/// };
/// ```
#[macro_export]
macro_rules! scoped_context {
    ($($key:expr => $value:expr),* $(,)?) => {{
        let mut context = serde_json::Map::new();
        $(
            context.insert($key.to_string(), serde_json::Value::from($value));
        )*
        $crate::log_context::scoped_context(context)
    }};
}

/// Add context values to the current layer
///
/// Usage:
/// ```
/// use vm_common::log_context;
/// log_context! {
///     "user_id" => "user123",
///     "duration_ms" => 1500
/// };
/// ```
#[macro_export]
macro_rules! log_context {
    ($($key:expr => $value:expr),* $(,)?) => {{
        $(
            $crate::log_context::add_to_current_context($key, $value);
        )*
    }};
}

/// Initialize the logging context system
/// This should be called once at application startup
pub fn init_context() {
    // Ensure we have at least one context layer
    LOG_CONTEXT.with(|stack| {
        let mut stack = stack.borrow_mut();
        if stack.is_empty() {
            stack.push(Map::new());
        }
    });
}

/// Clear all context (mainly for testing)
#[cfg(test)]
pub fn clear_context() {
    LOG_CONTEXT.with(|stack| {
        stack.borrow_mut().clear();
        stack.borrow_mut().push(Map::new());
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_scoped_context() {
        clear_context();

        // Test basic scoped context
        {
            let _guard = scoped_context! {
                "operation" => "test",
                "level" => 1
            };

            let context = current_context();
            assert_eq!(context.get("operation"), Some(&json!("test")));
            assert_eq!(context.get("level"), Some(&json!(1)));
        }

        // Context should be popped after guard is dropped
        let context = current_context();
        assert!(context.get("operation").is_none());
        assert!(context.get("level").is_none());
    }

    #[test]
    fn test_nested_context() {
        clear_context();

        let _guard1 = scoped_context! {
            "request_id" => "req123",
            "operation" => "create"
        };

        {
            let _guard2 = scoped_context! {
                "component" => "docker",
                "operation" => "build" // Should override parent
            };

            let context = current_context();
            assert_eq!(context.get("request_id"), Some(&json!("req123")));
            assert_eq!(context.get("operation"), Some(&json!("build"))); // Overridden
            assert_eq!(context.get("component"), Some(&json!("docker")));
        }

        // After inner scope, should revert to parent context
        let context = current_context();
        assert_eq!(context.get("request_id"), Some(&json!("req123")));
        assert_eq!(context.get("operation"), Some(&json!("create"))); // Reverted
        assert!(context.get("component").is_none()); // Removed
    }

    #[test]
    fn test_add_to_current_context() {
        clear_context();

        let _guard = scoped_context! {
            "base" => "value"
        };

        // Add more values to current context
        log_context! {
            "added" => "new_value",
            "number" => 42
        };

        let context = current_context();
        assert_eq!(context.get("base"), Some(&json!("value")));
        assert_eq!(context.get("added"), Some(&json!("new_value")));
        assert_eq!(context.get("number"), Some(&json!(42)));
    }
}
