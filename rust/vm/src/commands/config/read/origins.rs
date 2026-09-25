//! Field provenance for merged configuration reads.

use super::{find_project_config, nested_value, read_raw_config, read_scope};
use crate::cli::ConfigReadScope;
use crate::error::{VmError, VmResult};
use serde_yaml_ng as serde_yaml;
use std::path::PathBuf;
use vm_config::{config::VmConfig, AppConfig};

pub(super) struct ConfigOrigins {
    scope: ConfigReadScope,
    project_path: Option<PathBuf>,
    project: Option<serde_yaml::Value>,
    user_path: PathBuf,
    user: Option<serde_yaml::Value>,
    profile_name: Option<String>,
    preset_name: Option<String>,
    defaults: serde_yaml::Value,
    effective: Option<serde_yaml::Value>,
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
        let profile_name = loaded
            .as_ref()
            .and_then(|config| AppConfig::resolve_profile_name(config, profile.as_deref(), None));
        let preset_name = project
            .as_ref()
            .and_then(|value| nested_value(value, "preset"))
            .and_then(serde_yaml::Value::as_str)
            .map(str::to_string);
        let defaults = serde_yaml::to_value(VmConfig::default())
            .map_err(|error| VmError::config(error, "Cannot serialize default configuration"))?;
        let effective = if scope == ConfigReadScope::Effective {
            Some(read_scope(scope, project_path.clone(), profile)?.0)
        } else {
            None
        };
        Ok(Self {
            scope,
            project_path,
            project,
            user_path,
            user,
            profile_name,
            preset_name,
            defaults,
            effective,
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
        if field.starts_with("tools.")
            && self
                .user
                .as_ref()
                .is_some_and(|user| nested_value(user, field).is_some())
        {
            return self.user_path.display().to_string();
        }
        if let Some(preset) = &self.preset_name {
            if nested_value(&self.defaults, field)
                != self
                    .effective
                    .as_ref()
                    .and_then(|value| nested_value(value, field))
            {
                return format!("preset {preset}");
            }
        }
        "built-in defaults".to_string()
    }
}
