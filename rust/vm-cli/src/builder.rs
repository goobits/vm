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
