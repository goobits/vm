use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_yaml_ng::Value;
use vm_core::error::Result;
use vm_core::vm_println;

use crate::yaml::core::CoreOperations;

/// The validated document used by both preview and execution.
pub(super) struct ConfigEditPlan {
    path: PathBuf,
    scope: &'static str,
    before: Value,
    after: Value,
}

#[derive(Debug, Serialize)]
pub struct ConfigMutationReport {
    pub target: String,
    pub planned: bool,
    pub file_changes: Vec<ConfigFieldChange>,
    pub effective_changes: Vec<ConfigFieldChange>,
    pub preconditions: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct ConfigFieldChange {
    pub action: ConfigChangeAction,
    pub field: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigChangeAction {
    Add,
    Change,
    Remove,
}

impl ConfigEditPlan {
    pub(super) fn new(path: PathBuf, before: Value, after: Value, global: bool) -> Self {
        Self {
            path,
            scope: if global { "user" } else { "project" },
            before,
            after,
        }
    }

    pub(super) fn preview(&self) {
        self.preview_impl(&self.report(true, None));
    }

    pub(super) fn preview_with_effective(&self, before: &Value, after: &Value) {
        self.preview_impl(&self.report(true, Some((before, after))));
    }

    pub(super) fn report(
        &self,
        planned: bool,
        effective: Option<(&Value, &Value)>,
    ) -> ConfigMutationReport {
        let mut file_changes = Vec::new();
        collect_changes(Some(&self.before), Some(&self.after), "", &mut file_changes);
        let mut effective_changes = Vec::new();
        if let Some((before, after)) = effective {
            collect_changes(Some(before), Some(after), "", &mut effective_changes);
        }
        ConfigMutationReport {
            target: self.scope.to_string(),
            planned,
            file_changes,
            effective_changes,
            preconditions: vec!["resulting configuration is valid"],
        }
    }

    fn preview_impl(&self, report: &ConfigMutationReport) {
        vm_println!("Plan: update {}", self.path.display());
        vm_println!("Precondition: the configuration remains valid (checked)");
        print_changes("File changes", &report.file_changes);
        if !report.effective_changes.is_empty() {
            print_changes("Effective changes", &report.effective_changes);
        }
        vm_println!("No files were changed. Execution revalidates current state.");
    }

    pub(super) fn write(&self) -> Result<()> {
        CoreOperations::write_yaml_file(&self.path, &self.after)
    }
}

fn print_changes(label: &str, changes: &[ConfigFieldChange]) {
    if changes.is_empty() {
        vm_println!("{label}: none");
    } else {
        vm_println!("{label}:");
        for change in changes {
            let action = match change.action {
                ConfigChangeAction::Add => "add",
                ConfigChangeAction::Change => "change",
                ConfigChangeAction::Remove => "remove",
            };
            vm_println!("  {action} {}", change.field);
        }
    }
}

fn collect_changes(
    before: Option<&Value>,
    after: Option<&Value>,
    path: &str,
    out: &mut Vec<ConfigFieldChange>,
) {
    match (before, after) {
        (Some(Value::Mapping(old)), Some(Value::Mapping(new))) => {
            for (key, old_value) in old {
                let Some(key) = key.as_str() else { continue };
                let child = field_path(path, key);
                collect_changes(Some(old_value), new.get(key), &child, out);
            }
            for (key, new_value) in new {
                let Some(key) = key.as_str() else { continue };
                if !old.contains_key(key) {
                    let child = field_path(path, key);
                    collect_changes(None, Some(new_value), &child, out);
                }
            }
        }
        (Some(previous), Some(next)) if previous == next => {}
        (Some(_), Some(_)) => out.push(ConfigFieldChange {
            action: ConfigChangeAction::Change,
            field: path.to_string(),
        }),
        (None, Some(Value::Mapping(fields))) if !fields.is_empty() => {
            for (key, value) in fields {
                let Some(key) = key.as_str() else { continue };
                let child = field_path(path, key);
                collect_changes(None, Some(value), &child, out);
            }
        }
        (Some(Value::Mapping(fields)), None) if !fields.is_empty() => {
            for (key, value) in fields {
                let Some(key) = key.as_str() else { continue };
                let child = field_path(path, key);
                collect_changes(Some(value), None, &child, out);
            }
        }
        (None, Some(_)) => out.push(ConfigFieldChange {
            action: ConfigChangeAction::Add,
            field: path.to_string(),
        }),
        (Some(_), None) => out.push(ConfigFieldChange {
            action: ConfigChangeAction::Remove,
            field: path.to_string(),
        }),
        (None, None) => {}
    }
}

fn field_path(parent: &str, key: &str) -> String {
    if parent.is_empty() {
        key.to_string()
    } else {
        format!("{parent}.{key}")
    }
}

pub(super) fn read_document(path: &Path) -> Result<Value> {
    let content = std::fs::read_to_string(path)?;
    CoreOperations::parse_yaml_with_diagnostics(&content, &path.display().to_string())
}
