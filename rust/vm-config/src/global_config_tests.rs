use super::global_config::*;
use vm_core::error::Result;

#[test]
fn test_default_global_config() {
    let config = GlobalConfig::default();
    assert!(config.services.is_default());
    assert!(config.defaults.is_default());
    assert!(config.features.is_default());
    assert!(config.worktrees.is_default());
    assert!(config.backups.is_default());
}

#[test]
fn test_backup_settings_is_default() {
    let mut settings = BackupSettings::default();
    assert!(settings.is_default());
    settings.enabled = false;
    assert!(!settings.is_default());
}

#[test]
fn test_global_services_is_default() {
    let mut services = GlobalServices::default();
    assert!(services.is_default());
    services.docker_registry.enabled = true;
    assert!(!services.is_default());
}

#[test]
fn test_postgres_settings_is_default() {
    let mut settings = PostgresSettings::default();
    assert!(settings.is_default());
    settings.enabled = true;
    assert!(!settings.is_default());
}

#[test]
fn test_redis_settings_is_default() {
    let mut settings = RedisSettings::default();
    assert!(settings.is_default());
    settings.enabled = true;
    assert!(!settings.is_default());
}

#[test]
fn test_mongodb_settings_is_default() {
    let mut settings = MongoDBSettings::default();
    assert!(settings.is_default());
    settings.enabled = true;
    assert!(!settings.is_default());
}

#[test]
fn test_mysql_settings_is_default() {
    let mut settings = MySqlSettings::default();
    assert!(settings.is_default());
    settings.enabled = true;
    assert!(!settings.is_default());
}

#[test]
fn test_docker_registry_settings_is_default() {
    let mut settings = DockerRegistrySettings::default();
    assert!(settings.is_default());
    settings.enabled = true;
    assert!(!settings.is_default());
}

#[test]
fn test_auth_proxy_settings_is_default() {
    let mut settings = AuthProxySettings::default();
    assert!(settings.is_default());
    settings.enabled = true;
    assert!(!settings.is_default());
}

#[test]
fn test_package_registry_settings_is_default() {
    let mut settings = PackageRegistrySettings::default();
    assert!(settings.is_default());
    settings.enabled = true;
    assert!(!settings.is_default());
}

#[test]
fn test_global_defaults_is_default() {
    let mut defaults = GlobalDefaults::default();
    assert!(defaults.is_default());
    defaults.provider = Some("docker".to_string());
    assert!(!defaults.is_default());
}

#[test]
fn test_global_features_is_default() {
    let mut features = GlobalFeatures::default();
    assert!(features.is_default());
    features.telemetry = true;
    assert!(!features.is_default());
}

#[test]
fn test_worktrees_global_settings_is_default() {
    let mut settings = WorktreesGlobalSettings::default();
    assert!(settings.is_default());
    settings.enabled = false;
    assert!(!settings.is_default());
}

#[test]
fn test_serialization_deserialization_cycle() {
    let mut config = GlobalConfig::default();
    config.services.postgresql.enabled = true;
    config.defaults.cpus = Some(4);

    let yaml = serde_yaml_ng::to_string(&config).unwrap();
    let deserialized: GlobalConfig = serde_yaml_ng::from_str(&yaml).unwrap();

    assert!(deserialized.services.postgresql.enabled);
    assert_eq!(deserialized.defaults.cpus, Some(4));
}

#[test]
fn test_load_save_cycle() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let config_path = temp_dir.path().join("config.yaml");

    let mut config = GlobalConfig::default();
    config.features.telemetry = true;

    config.save_to_path(&config_path)?;

    let loaded_config = GlobalConfig::load_from_path(&config_path)?;
    assert!(loaded_config.features.telemetry);

    Ok(())
}
