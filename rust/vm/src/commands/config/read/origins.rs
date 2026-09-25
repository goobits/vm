//! Field provenance for merged configuration reads.

use super::{collect_field_paths, find_project_config, nested_value, read_raw_config};
use crate::cli::ConfigReadScope;
use crate::error::{VmError, VmResult};
use serde_yaml_ng as serde_yaml;
use std::collections::BTreeSet;
use std::path::PathBuf;
use vm_config::{config::VmConfig, AppConfig, PresetDetector};

pub(super) struct ConfigOrigins {
    scope: ConfigReadScope,
    project_path: Option<PathBuf>,
    project: Option<serde_yaml::Value>,
    user_path: PathBuf,
    user: Option<serde_yaml::Value>,
    profile_name: Option<String>,
    preset_name: Option<String>,
    preset_fields: BTreeSet<String>,
}

impl ConfigOrigins {
    pub(super) fn load(
        scope: ConfigReadScope,
        config_path: Option<PathBuf>,
        profile: Option<String>,
    ) -> VmResult<Self> {
        let user_path = vm_core::user_paths::global_config_path()?;
        let user = if scope == ConfigReadScope::Project {
            None
        } else {
            user_path
                .is_file()
                .then(|| read_raw_config(user_path.clone()).map(|(value, _)| value))
                .transpose()?
        };
        let project_path = if scope == ConfigReadScope::User {
            None
        } else {
            match config_path {
                Some(path) => Some(path),
                None => find_project_config().ok(),
            }
        };
        let project = project_path
            .as_ref()
            .filter(|path| path.is_file())
            .map(|path| read_raw_config(path.clone()).map(|(value, _)| value))
            .transpose()?;
        let loaded = if scope == ConfigReadScope::Effective {
            project_path
                .as_ref()
                .filter(|path| path.is_file())
                .map(|path| VmConfig::load(Some(path.clone())))
                .transpose()?
        } else {
            None
        };
        let profile_name = loaded.as_ref().and_then(|config| {
            let mut with_user_provider = config.clone();
            if with_user_provider.provider.is_none() {
                with_user_provider.provider = user
                    .as_ref()
                    .and_then(|value| nested_value(value, "defaults.provider"))
                    .and_then(serde_yaml::Value::as_str)
                    .map(Into::into);
            }
            AppConfig::resolve_profile_name(&with_user_provider, profile.as_deref(), None)
        });
        let preset_name = project
            .as_ref()
            .and_then(|value| nested_value(value, "preset"))
            .and_then(serde_yaml::Value::as_str)
            .map(str::to_string);
        let preset_fields = if scope == ConfigReadScope::Effective {
            collect_preset_fields(
                preset_name.as_deref(),
                project_path.as_ref(),
                loaded.as_ref(),
            )?
        } else {
            BTreeSet::new()
        };
        Ok(Self {
            scope,
            project_path,
            project,
            user_path,
            user,
            profile_name,
            preset_name,
            preset_fields,
        })
    }

    pub(super) fn source_for(&self, field: &str) -> String {
        match self.scope {
            ConfigReadScope::Project => {
                return self
                    .project_path
                    .as_ref()
                    .map_or_else(|| "project".to_string(), |path| path.display().to_string())
            }
            ConfigReadScope::User => return self.user_path.display().to_string(),
            ConfigReadScope::Effective => {}
        }
        if let (Some(profile), Some(project)) = (&self.profile_name, &self.project) {
            let profile_field = format!("profiles.{profile}.{field}");
            if nested_value(project, &profile_field).is_some() {
                return format!(
                    "profile {profile} in {}",
                    self.project_path.as_ref().unwrap().display()
                );
            }
        }
        if let (Some(profile), Some(preset)) = (&self.profile_name, &self.preset_name) {
            let profile_field = format!("profiles.{profile}.{field}");
            if self.preset_fields.contains(&profile_field) {
                return format!("profile {profile} from preset {preset}");
            }
        }
        if self
            .project
            .as_ref()
            .is_some_and(|project| nested_value(project, field).is_some())
        {
            return self.project_path.as_ref().unwrap().display().to_string();
        }
        if let Some(name) = field
            .strip_prefix("tools.")
            .and_then(|tail| tail.split('.').next())
        {
            if self
                .project
                .as_ref()
                .is_some_and(|project| nested_value(project, &format!("tools.{name}")).is_some())
            {
                return self.project_path.as_ref().unwrap().display().to_string();
            }
        }
        if let Some(preset) = &self.preset_name {
            if self.preset_fields.contains(field)
                || self
                    .preset_fields
                    .iter()
                    .any(|path| path.starts_with(&format!("{field}.")))
            {
                return format!("preset {preset}");
            }
        }
        let user_field = match field {
            "provider" => Some("defaults.provider".to_string()),
            "vm.memory" => Some("defaults.memory".to_string()),
            "vm.cpus" => Some("defaults.cpus".to_string()),
            "vm.user" => Some("defaults.user".to_string()),
            path if path.starts_with("terminal.") => Some(format!("defaults.{path}")),
            path if path.starts_with("tools.") => Some(path.to_string()),
            _ => None,
        };
        if user_field.as_ref().is_some_and(|path| {
            self.user
                .as_ref()
                .is_some_and(|user| nested_value(user, path).is_some())
        }) {
            return self.user_path.display().to_string();
        }
        "built-in defaults".to_string()
    }
}

fn collect_preset_fields(
    names: Option<&str>,
    path: Option<&PathBuf>,
    loaded: Option<&VmConfig>,
) -> VmResult<BTreeSet<String>> {
    let (Some(names), Some(path)) = (names, path) else {
        return Ok(BTreeSet::new());
    };
    let detector = PresetDetector::new(path.parent().unwrap_or(path).to_path_buf());
    let defaults = serde_yaml::to_value(VmConfig::default())
        .map_err(|error| VmError::config(error, "Cannot serialize default configuration"))?;
    let range = loaded
        .and_then(|config| config.ports.range.as_ref())
        .and_then(|range| (range.len() == 2).then(|| format!("{}-{}", range[0], range[1])));
    let mut fields = BTreeSet::new();
    for name in names
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        let preset = detector
            .load_preset_resolved(name, range.as_deref())
            .map_err(VmError::from)?;
        let value = serde_yaml::to_value(preset)
            .map_err(|error| VmError::config(error, "Cannot serialize preset"))?;
        let mut paths = Vec::new();
        collect_field_paths(&value, "", &mut paths);
        fields.extend(
            paths
                .into_iter()
                .filter(|field| nested_value(&value, field) != nested_value(&defaults, field)),
        );
    }
    Ok(fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_prioritizes_profile_project_preset_and_user_defaults() {
        let project: serde_yaml::Value = serde_yaml::from_str(
            "vm:\n  user: project\nprofiles:\n  fast:\n    vm:\n      memory: 8192\n",
        )
        .unwrap();
        let user: serde_yaml::Value = serde_yaml::from_str("defaults:\n  memory: 4096\n  cpus: 4\n  terminal:\n    theme: nord\ntools:\n  codex:\n    version: 1.2.3\n").unwrap();
        let origins = ConfigOrigins {
            scope: ConfigReadScope::Effective,
            project_path: Some(PathBuf::from("/project/vm.yaml")),
            project: Some(project),
            user_path: PathBuf::from("/user/.vm/config.yaml"),
            user: Some(user),
            profile_name: Some("fast".into()),
            preset_name: Some("vibe-tart".into()),
            preset_fields: BTreeSet::from([
                "vm.image".to_string(),
                "profiles.fast.terminal.emoji".to_string(),
            ]),
        };
        assert!(origins.source_for("vm.memory").starts_with("profile fast"));
        assert_eq!(origins.source_for("vm.user"), "/project/vm.yaml");
        assert_eq!(origins.source_for("vm.image"), "preset vibe-tart");
        assert_eq!(
            origins.source_for("terminal.emoji"),
            "profile fast from preset vibe-tart"
        );
        assert_eq!(origins.source_for("vm.cpus"), "/user/.vm/config.yaml");
        assert_eq!(
            origins.source_for("terminal.theme"),
            "/user/.vm/config.yaml"
        );
        assert_eq!(
            origins.source_for("tools.codex.version"),
            "/user/.vm/config.yaml"
        );
        assert_eq!(origins.source_for("os"), "built-in defaults");
    }
}
