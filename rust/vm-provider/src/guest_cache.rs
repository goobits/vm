use vm_config::config::VmConfig;

use crate::stable_name::stable_name_component;

#[cfg(feature = "docker")]
pub(crate) const CACHE_ENV_CONFIG_KEY: &str = "_vm_cache_environment";

/// Provider-neutral cache locations for tools that otherwise write to shared source mounts
/// or disposable container layers.
pub(crate) struct GuestCachePolicy {
    project_key: String,
}

impl GuestCachePolicy {
    pub(crate) fn from_config(config: &VmConfig) -> Self {
        let project_name = config
            .project
            .as_ref()
            .and_then(|project| project.name.as_deref())
            .unwrap_or("project");
        Self::new(project_name)
    }

    pub(crate) fn new(project_name: &str) -> Self {
        let project_key = stable_name_component(project_name);

        Self {
            project_key: if project_key.is_empty() {
                "project".to_string()
            } else {
                project_key
            },
        }
    }

    #[cfg(any(feature = "tart", test))]
    pub(crate) fn shell_exports(&self) -> String {
        let mut exports =
            vec![r#"export XDG_CACHE_HOME="${XDG_CACHE_HOME:-$HOME/.cache}""#.to_string()];
        exports.extend(
            self.paths("${XDG_CACHE_HOME}")
                .into_iter()
                .map(|(variable, path)| format!(r#"export {variable}="${{{variable}:-{path}}}""#)),
        );
        exports.join("\n")
    }

    #[cfg(any(feature = "docker", test))]
    pub(crate) fn container_environment(
        &self,
        home_dir: &str,
        config: &VmConfig,
    ) -> Vec<(String, String)> {
        let cache_home = config
            .environment
            .get("XDG_CACHE_HOME")
            .filter(|path| path.starts_with('/'))
            .cloned()
            .unwrap_or_else(|| format!("{home_dir}/.cache"));
        let mut environment = Vec::new();
        if !config.environment.contains_key("XDG_CACHE_HOME") {
            environment.push(("XDG_CACHE_HOME".to_string(), cache_home.clone()));
        }
        environment.extend(
            self.paths(&cache_home)
                .into_iter()
                .filter(|(variable, _)| !config.environment.contains_key(*variable))
                .map(|(variable, path)| (variable.to_string(), path)),
        );
        environment
    }

    fn paths(&self, cache_home: &str) -> Vec<(&'static str, String)> {
        vec![
            (
                "CARGO_TARGET_DIR",
                format!("{cache_home}/vm/cargo-target/{}", self.project_key),
            ),
            ("COREPACK_HOME", format!("{cache_home}/node/corepack")),
            ("GOCACHE", format!("{cache_home}/go/build")),
            ("GOMODCACHE", format!("{cache_home}/go/mod")),
            ("npm_config_cache", format!("{cache_home}/node/npm")),
            ("PIP_CACHE_DIR", format!("{cache_home}/python/pip")),
            (
                "PLAYWRIGHT_BROWSERS_PATH",
                format!("{cache_home}/ms-playwright"),
            ),
            (
                "PYTHONPYCACHEPREFIX",
                format!("{cache_home}/python/pycache"),
            ),
            ("UV_CACHE_DIR", format!("{cache_home}/python/uv")),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::GuestCachePolicy;
    use vm_config::config::{ProjectConfig, VmConfig};

    #[test]
    fn cache_paths_are_project_scoped_and_overrideable() {
        let mut config = VmConfig {
            project: Some(ProjectConfig {
                name: Some("code atlas/feature".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        config
            .environment
            .insert("PIP_CACHE_DIR".to_string(), "/custom/pip".to_string());
        let policy = GuestCachePolicy::from_config(&config);

        let environment = policy.container_environment("/home/developer", &config);

        assert!(environment.iter().any(|(name, path)| {
            name == "CARGO_TARGET_DIR"
                && path == "/home/developer/.cache/vm/cargo-target/code_atlas_feature"
        }));
        assert!(!environment.iter().any(|(name, _)| name == "PIP_CACHE_DIR"));
        assert_eq!(
            environment
                .iter()
                .filter(|(name, _)| name == "XDG_CACHE_HOME")
                .count(),
            1
        );
    }

    #[test]
    fn shell_defaults_preserve_explicit_guest_values() {
        let exports = GuestCachePolicy::new("codeatlas").shell_exports();

        assert!(exports.contains("${XDG_CACHE_HOME:-$HOME/.cache}"));
        assert!(
            exports.contains("${CARGO_TARGET_DIR:-${XDG_CACHE_HOME}/vm/cargo-target/codeatlas}")
        );
        assert!(exports.contains("${PLAYWRIGHT_BROWSERS_PATH:-"));
    }
}
