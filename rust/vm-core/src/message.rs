//! Lightweight templated string builder used by the user-facing message
//! formatting macro [`crate::msg`].
//!
//! `MessageBuilder` substitutes `{name}` placeholders in a `&'static str`
//! template with values supplied at runtime. It deliberately stays tiny — it's
//! the foundation for `msg!`, which is consumed by every CLI-adjacent crate.

use std::collections::HashMap;

pub struct MessageBuilder {
    template: &'static str,
    vars: HashMap<&'static str, String>,
}

impl MessageBuilder {
    pub fn new(template: &'static str) -> Self {
        Self {
            template,
            vars: HashMap::new(),
        }
    }

    pub fn var(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.vars.insert(key, value.into());
        self
    }

    pub fn build(self) -> String {
        let mut result = self.template.to_string();
        for (key, value) in self.vars {
            result = result.replace(&format!("{{{key}}}"), &value);
        }
        result
    }
}

/// Format a message template with named placeholders.
///
/// ```ignore
/// use vm_core::msg;
/// let s = msg!("Hello, {name}!", name = "world");
/// assert_eq!(s, "Hello, world!");
/// ```
#[macro_export]
macro_rules! msg {
    ($template:expr) => {
        $crate::message::MessageBuilder::new($template).build()
    };
    ($template:expr, $($key:ident = $value:expr),+ $(,)?) => {
        {
            let mut builder = $crate::message::MessageBuilder::new($template);
            $(
                builder = builder.var(stringify!($key), $value);
            )+
            builder.build()
        }
    };
}
