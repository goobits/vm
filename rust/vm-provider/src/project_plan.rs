use serde::Serialize;
use std::path::Path;
use vm_config::{config::VmConfig, detector::ProjectFacts};

#[cfg(feature = "docker")]
pub(crate) const PROJECT_PLAN_CONFIG_KEY: &str = "_vm_project_plan";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum NodePackageManager {
    Pnpm,
    Npm,
}

#[cfg(feature = "tart")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrimaryRuntime {
    Node,
    Python,
    Ruby,
    Rust,
    Go,
    Unknown,
}

#[cfg(feature = "tart")]
impl PrimaryRuntime {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Node => "nodejs",
            Self::Python => "python",
            Self::Ruby => "ruby",
            Self::Rust => "rust",
            Self::Go => "go",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct InstallPlan {
    pub(crate) node_runtime: bool,
    pub(crate) node_dependencies: Option<NodePackageManager>,
    pub(crate) playwright_browsers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ProjectPlan {
    version: u8,
    pub(crate) facts: ProjectFacts,
    pub(crate) installs: InstallPlan,
}

impl ProjectPlan {
    pub(crate) fn detect(project_dir: &Path, config: &VmConfig) -> Self {
        let facts = ProjectFacts::detect(project_dir);
        let bootstrap_dependencies = config
            .bootstrap
            .as_ref()
            .map(|bootstrap| bootstrap.dependencies)
            .unwrap_or(true);
        let node_package_manager = if facts.pnpm_lock {
            Some(NodePackageManager::Pnpm)
        } else if facts.npm_lock {
            Some(NodePackageManager::Npm)
        } else {
            None
        };
        let node_dependencies = if bootstrap_dependencies && facts.package_json {
            node_package_manager
        } else {
            None
        };
        let mut playwright_browsers = config
            .bootstrap
            .as_ref()
            .map(|bootstrap| bootstrap.playwright.browsers.clone())
            .unwrap_or_default();
        playwright_browsers.sort();
        playwright_browsers.dedup();

        let installs = InstallPlan {
            node_runtime: facts.package_json || !config.npm_packages.is_empty(),
            node_dependencies,
            playwright_browsers,
        };

        Self {
            version: 1,
            facts,
            installs,
        }
    }

    #[cfg(feature = "tart")]
    pub(crate) fn primary_runtime(&self) -> PrimaryRuntime {
        if self.facts.package_json {
            PrimaryRuntime::Node
        } else if self.facts.has_python_project() {
            PrimaryRuntime::Python
        } else if self.facts.gemfile {
            PrimaryRuntime::Ruby
        } else if self.facts.cargo_toml {
            PrimaryRuntime::Rust
        } else if self.facts.go_mod {
            PrimaryRuntime::Go
        } else {
            PrimaryRuntime::Unknown
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "tart")]
    use super::PrimaryRuntime;
    use super::{NodePackageManager, ProjectPlan};
    use vm_config::config::{BootstrapConfig, PlaywrightBootstrapConfig, VmConfig};

    #[test]
    fn produces_one_stable_plan_for_multi_runtime_projects() {
        let project = tempfile::tempdir().unwrap();
        for marker in ["package.json", "pnpm-lock.yaml", "Cargo.toml"] {
            std::fs::write(project.path().join(marker), "").unwrap();
        }
        let config = VmConfig {
            bootstrap: Some(BootstrapConfig {
                dependencies: true,
                playwright: PlaywrightBootstrapConfig {
                    browsers: vec!["webkit".to_string(), "chromium".to_string()],
                },
            }),
            ..Default::default()
        };

        let plan = ProjectPlan::detect(project.path(), &config);

        #[cfg(feature = "tart")]
        assert_eq!(plan.primary_runtime(), PrimaryRuntime::Node);
        assert_eq!(
            plan.installs.node_dependencies,
            Some(NodePackageManager::Pnpm)
        );
        assert_eq!(plan.installs.playwright_browsers, ["chromium", "webkit"]);
        let json = serde_json::to_value(plan).unwrap();
        assert_eq!(json["version"], 1);
        assert_eq!(json["installs"]["node_dependencies"], "pnpm");
    }

    #[test]
    fn dependency_bootstrap_can_be_disabled_without_hiding_the_runtime() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("package.json"), "{}").unwrap();
        std::fs::write(project.path().join("package-lock.json"), "{}").unwrap();
        let config = VmConfig {
            bootstrap: Some(BootstrapConfig {
                dependencies: false,
                ..Default::default()
            }),
            ..Default::default()
        };

        let plan = ProjectPlan::detect(project.path(), &config);

        assert!(plan.installs.node_runtime);
        assert_eq!(plan.installs.node_dependencies, None);
    }
}
