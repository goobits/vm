use crate::cli::EnvironmentKind;
use crate::error::{VmError, VmResult};
use dialoguer::{theme::ColorfulTheme, Select};
use std::io::IsTerminal;
use std::path::PathBuf;
use vm_config::config::VmConfig;

pub(super) struct ResolvedEnvironment {
    pub(super) provider_override: Option<String>,
    pub(super) profile: Option<String>,
    pub(super) target: Option<String>,
}

impl ResolvedEnvironment {
    fn new(
        provider_override: Option<String>,
        profile: Option<String>,
        target: Option<String>,
    ) -> Self {
        Self {
            provider_override,
            profile,
            target,
        }
    }
}

fn resolve_noninteractive(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    environment: Option<String>,
) -> ResolvedEnvironment {
    match environment.as_deref() {
        Some("mac") => ResolvedEnvironment::new(
            Some(EnvironmentKind::Mac.default_provider().to_string()),
            profile.or_else(|| mac_profile(config_path)),
            Some("mac".to_string()),
        ),
        Some("linux") => ResolvedEnvironment::new(
            Some(EnvironmentKind::Linux.default_provider().to_string()),
            None,
            None,
        ),
        Some("container") => ResolvedEnvironment::new(
            Some(EnvironmentKind::Container.default_provider().to_string()),
            None,
            None,
        ),
        Some(environment) => {
            if profile.is_none() && profile_exists(config_path.clone(), environment) {
                let selected_profile = environment.to_string();
                return ResolvedEnvironment::new(
                    None,
                    Some(selected_profile.clone()),
                    target_for_profile(config_path, &selected_profile),
                );
            }

            ResolvedEnvironment::new(None, profile, Some(environment.to_string()))
        }
        None => {
            let target = profile
                .as_deref()
                .and_then(|profile| target_for_profile(config_path, profile));
            ResolvedEnvironment::new(None, profile, target)
        }
    }
}

fn profile_exists(config_path: Option<PathBuf>, profile: &str) -> bool {
    VmConfig::load(config_path)
        .ok()
        .and_then(|config| config.profiles)
        .is_some_and(|profiles| profiles.contains_key(profile))
}

fn target_for_profile(config_path: Option<PathBuf>, profile: &str) -> Option<String> {
    let config = VmConfig::load(config_path).ok()?;
    let profile_config = config.profiles.as_ref()?.get(profile)?;
    if profile_is_macos(Some(profile_config)) {
        Some("mac".to_string())
    } else {
        None
    }
}

pub(super) fn resolve_environment(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    environment: Option<String>,
) -> VmResult<ResolvedEnvironment> {
    if environment.is_some() || profile.is_some() || !std::io::stdin().is_terminal() {
        return Ok(resolve_noninteractive(config_path, profile, environment));
    }

    let config = VmConfig::load(config_path.clone()).map_err(VmError::from)?;
    let Some(profiles) = config
        .profiles
        .as_ref()
        .filter(|profiles| profiles.len() > 1)
    else {
        return Ok(resolve_noninteractive(config_path, None, None));
    };

    let choices: Vec<(String, String, Option<String>)> = profiles
        .iter()
        .map(|(name, profile_config)| {
            let kind = profile_label(profile_config);
            let default_marker = if config.default_profile.as_deref() == Some(name.as_str()) {
                " default"
            } else {
                ""
            };
            (
                name.clone(),
                format!("{kind} ({name} profile{default_marker})"),
                if profile_is_macos(Some(profile_config)) {
                    Some("mac".to_string())
                } else {
                    None
                },
            )
        })
        .collect();

    let labels: Vec<&str> = choices.iter().map(|(_, label, _)| label.as_str()).collect();
    let default_index = choices
        .iter()
        .position(|(name, _, _)| config.default_profile.as_deref() == Some(name.as_str()))
        .unwrap_or(0);
    let selected = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Which environment?")
        .items(&labels)
        .default(default_index)
        .interact()
        .map_err(|e| VmError::general(e, "Failed to read environment selection"))?;

    Ok(ResolvedEnvironment {
        provider_override: None,
        profile: Some(choices[selected].0.clone()),
        target: choices[selected].2.clone(),
    })
}

fn profile_label(profile: &VmConfig) -> &'static str {
    match profile.provider.as_deref() {
        Some("docker") | Some("podman") => "Container",
        Some("tart") => {
            if profile_is_macos(Some(profile)) {
                "macOS"
            } else {
                "Linux"
            }
        }
        _ => "Environment",
    }
}

pub(super) fn mac_profile(config_path: Option<PathBuf>) -> Option<String> {
    let profiles = VmConfig::load(config_path).ok()?.profiles?;

    ["macos", "mac", "tart"]
        .iter()
        .find(|name| profile_is_macos(profiles.get(**name)))
        .map(|name| (*name).to_string())
        .or_else(|| {
            profiles
                .iter()
                .find(|(_, profile)| profile_is_macos(Some(profile)))
                .map(|(name, _)| name.to_string())
        })
}

fn profile_is_macos(profile: Option<&VmConfig>) -> bool {
    profile
        .and_then(|profile| profile.tart.as_ref())
        .and_then(|tart| tart.guest_os.as_deref())
        .is_some_and(|guest_os| guest_os.eq_ignore_ascii_case("macos"))
}

#[cfg(test)]
mod tests {
    use super::{resolve_noninteractive, ResolvedEnvironment};

    fn assert_resolved(
        resolved: ResolvedEnvironment,
        provider_override: Option<&str>,
        profile: Option<&str>,
        target: Option<&str>,
    ) {
        assert_eq!(resolved.provider_override.as_deref(), provider_override);
        assert_eq!(resolved.profile.as_deref(), profile);
        assert_eq!(resolved.target.as_deref(), target);
    }

    #[test]
    fn resolver_accepts_kind_words() {
        let missing_config =
            Some(std::env::temp_dir().join("vm-missing-config-for-shell-test.yaml"));
        assert_resolved(
            resolve_noninteractive(missing_config, None, Some("mac".into())),
            Some("tart"),
            None,
            Some("mac"),
        );
        assert_resolved(
            resolve_noninteractive(None, None, Some("backend".into())),
            None,
            None,
            Some("backend"),
        );
    }

    #[test]
    fn resolver_uses_macos_tart_profile() {
        let config_path =
            std::env::temp_dir().join(format!("vm-macos-tart-profile-{}.yaml", std::process::id()));
        std::fs::write(
            &config_path,
            r#"
version: '2.0'
profiles:
  tart:
    provider: tart
    tart:
      guest_os: macos
"#,
        )
        .expect("write test config");

        assert_resolved(
            resolve_noninteractive(Some(config_path.clone()), None, Some("mac".into())),
            Some("tart"),
            Some("tart"),
            Some("mac"),
        );

        let _ = std::fs::remove_file(config_path);
    }

    #[test]
    fn resolver_targets_mac_instance_for_macos_profile() {
        let config_path = std::env::temp_dir().join(format!(
            "vm-shell-macos-profile-target-{}.yaml",
            std::process::id()
        ));
        std::fs::write(
            &config_path,
            r#"
version: '2.0'
profiles:
  tart:
    provider: tart
    tart:
      guest_os: macos
"#,
        )
        .expect("write test config");

        assert_resolved(
            resolve_noninteractive(Some(config_path.clone()), Some("tart".into()), None),
            None,
            Some("tart"),
            Some("mac"),
        );
        assert_resolved(
            resolve_noninteractive(Some(config_path.clone()), None, Some("tart".into())),
            None,
            Some("tart"),
            Some("mac"),
        );

        let _ = std::fs::remove_file(config_path);
    }

    #[test]
    fn resolver_does_not_target_instance_for_container_profile() {
        let config_path = std::env::temp_dir().join(format!(
            "vm-shell-container-profile-target-{}.yaml",
            std::process::id()
        ));
        std::fs::write(
            &config_path,
            r#"
version: '2.0'
profiles:
  docker:
    provider: docker
"#,
        )
        .expect("write test config");

        assert_resolved(
            resolve_noninteractive(Some(config_path.clone()), Some("docker".into()), None),
            None,
            Some("docker"),
            None,
        );
        assert_resolved(
            resolve_noninteractive(Some(config_path.clone()), None, Some("docker".into())),
            None,
            Some("docker"),
            None,
        );

        let _ = std::fs::remove_file(config_path);
    }
}
