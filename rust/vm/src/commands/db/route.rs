//! Resolve database commands to one project environment and configured service.

use std::path::PathBuf;

use sha2::{Digest, Sha256};
use vm_config::{config::VmConfig, AppConfig};

use crate::error::{VmError, VmResult};

#[derive(Clone)]
pub(crate) struct DbRoute {
    pub database: String,
    pub environment: String,
    pub engine: String,
    pub container: String,
    pub user: String,
    pub configured_password: Option<String>,
    pub backup_namespace: String,
}

impl DbRoute {
    pub fn load(
        config_path: Option<PathBuf>,
        profile: Option<String>,
        requested_environment: Option<String>,
    ) -> VmResult<Self> {
        let app = AppConfig::load(config_path, profile, None)?;
        super::super::command_context::require_project_config(&app.vm)?;
        let selected =
            super::super::declarations::selected_name(&app.vm, requested_environment.as_deref())?;
        if let Some(name) = &selected {
            if !app.vm.environments.contains_key(name) {
                return Err(VmError::validation(
                    format!("Environment '{name}' is not declared in this project"),
                    Some("Run `vm list` to see declared environments"),
                ));
            }
        }
        let config = selected
            .as_deref()
            .and_then(|name| app.vm.environments.get(name))
            .map_or_else(
                || app.vm.clone(),
                |declaration| declaration.apply_to(&app.vm),
            );
        Self::for_config(
            &config,
            selected.as_deref(),
            app.global.container_provider().as_str(),
        )
    }

    pub(crate) fn for_config(
        config: &VmConfig,
        environment: Option<&str>,
        engine: &str,
    ) -> VmResult<Self> {
        let service = config
            .services
            .get("postgresql")
            .filter(|service| service.enabled)
            .ok_or_else(|| {
                VmError::validation(
                    "PostgreSQL is not enabled for the selected environment",
                    Some("Configure services.postgresql.enabled in vm.yaml"),
                )
            })?;
        let project = config
            .project
            .as_ref()
            .and_then(|project| project.name.as_deref())
            .ok_or_else(|| {
                VmError::validation(
                    "A project name is required for database routing",
                    None::<String>,
                )
            })?;
        let database = service
            .resolved_database(project, environment)
            .map_err(|error| {
                VmError::validation(
                    error,
                    Some("Use project.name or environment.name placeholders"),
                )
            })?;
        super::backup::validate_backup_component(&database)?;
        let engine = config
            .provider
            .as_ref()
            .map_or(engine, |provider| provider.as_str());
        if !matches!(engine, "docker" | "podman") {
            return Err(VmError::validation(
                format!("PostgreSQL database commands require a Docker or Podman environment; '{engine}' is unsupported"),
                None::<String>,
            ));
        }
        let owning_path = config
            .owning_config_path()
            .ok_or_else(|| {
                VmError::validation(
                    "A project configuration is required for database routing",
                    None::<String>,
                )
            })?
            .canonicalize()?;
        let digest = Sha256::digest(owning_path.to_string_lossy().as_bytes());
        let project_key = digest[..16]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let container = environment.map_or_else(
            || format!("{project}-postgres"),
            |name| format!("{project}-{name}-postgres"),
        );
        let environment = environment.unwrap_or("default").to_string();
        super::backup::validate_backup_component(&environment)?;
        Ok(Self {
            database,
            environment: environment.clone(),
            engine: engine.to_string(),
            container,
            user: service
                .user
                .clone()
                .unwrap_or_else(|| "postgres".to_string()),
            configured_password: service.password.clone(),
            backup_namespace: format!("{project_key}/{environment}"),
        })
    }

    pub fn validate_database_name(name: &str) -> VmResult<()> {
        super::backup::validate_backup_component(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_environments_route_to_their_configured_databases_and_backup_namespaces() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vm.yaml");
        std::fs::write(
            &path,
            r#"version: '2.0'
project:
  name: demo
provider: docker
services:
  postgresql:
    enabled: true
    database: '{{ project.name }}_base'
environments:
  dev:
    provider: docker
    image: postgres:17
    services:
      postgresql:
        enabled: true
        database: '{{project.name}}_dev'
        user: dev_admin
        password: dev_secret
  test:
    provider: docker
    image: postgres:17
    services:
      postgresql:
        enabled: true
        database: '{{ environment.name }}_db'
"#,
        )
        .unwrap();
        let config = VmConfig::load(Some(path)).unwrap();
        let dev = config.environments["dev"].apply_to(&config);
        let test = config.environments["test"].apply_to(&config);
        let dev = DbRoute::for_config(&dev, Some("dev"), "docker").unwrap();
        let test = DbRoute::for_config(&test, Some("test"), "docker").unwrap();
        assert_eq!(dev.database, "demo_dev");
        assert_eq!(test.database, "test_db");
        assert_eq!(dev.container, "demo-dev-postgres");
        assert_eq!(test.container, "demo-test-postgres");
        assert_eq!(dev.engine, "docker");
        assert_eq!(dev.user, "dev_admin");
        assert_eq!(dev.configured_password.as_deref(), Some("dev_secret"));
        assert_eq!(
            DbRoute::for_config(&config, None, "docker")
                .unwrap()
                .container,
            "demo-postgres"
        );
        assert_ne!(dev.backup_namespace, test.backup_namespace);
        assert!(DbRoute::validate_database_name("demo_test").is_ok());
        assert!(DbRoute::validate_database_name("../other").is_err());

        let other_directory = tempfile::tempdir().unwrap();
        let other_path = other_directory.path().join("vm.yaml");
        std::fs::copy(directory.path().join("vm.yaml"), &other_path).unwrap();
        let other = VmConfig::load(Some(other_path)).unwrap();
        let other_dev = other.environments["dev"].apply_to(&other);
        let other_route = DbRoute::for_config(&other_dev, Some("dev"), "docker").unwrap();
        assert_ne!(dev.backup_namespace, other_route.backup_namespace);
    }
}
