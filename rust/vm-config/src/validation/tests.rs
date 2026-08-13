use super::{validate_config, ValidationMode};
use crate::config::{ProjectConfig, VmConfig, VmSettings};

fn valid_config() -> VmConfig {
    VmConfig {
        provider: Some("docker".to_string()),
        project: Some(ProjectConfig {
            name: Some("validation-test".to_string()),
            ..Default::default()
        }),
        vm: Some(VmSettings {
            user: Some("developer".to_string()),
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[test]
fn every_mode_runs_structural_validation() {
    let mut config = valid_config();
    config.provider = Some("invalid".to_string());

    for mode in [
        ValidationMode::Static,
        ValidationMode::Create {
            reusable_host_ports: &[],
        },
        ValidationMode::Recreate,
    ] {
        assert!(validate_config(&config, mode).unwrap().has_errors());
    }
}
