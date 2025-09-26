use crate::msg;

pub struct ErrorContext {
    pub operation: &'static str,
    pub vm_name: Option<String>,
    pub suggestions: &'static [&'static str],
}

impl ErrorContext {
    pub fn display(&self) -> String {
        let mut result = String::new();

        if let Some(name) = &self.vm_name {
            result.push_str(&msg!(
                "❌ Failed to {} '{}'",
                operation = self.operation,
                name = name
            ));
        } else {
            result.push_str(&msg!("❌ {} failed", operation = self.operation));
        }

        if !self.suggestions.is_empty() {
            result.push_str("\n\n💡 Try:");
            for suggestion in self.suggestions {
                result.push_str(&format!("\n  • {}", suggestion));
            }
        }
        result
    }
}
