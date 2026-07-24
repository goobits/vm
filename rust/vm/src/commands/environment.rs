use crate::cli::EnvironmentKind;
use crate::error::{VmError, VmResult};
use dialoguer::{theme::ColorfulTheme, Select};
use std::io::IsTerminal;
use std::path::PathBuf;
use vm_config::{config::VmConfig, AppConfig};

#[derive(Debug)]
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

fn selected_profile(
    config_path: Option<PathBuf>,
    explicit_profile: Option<String>,
    provider_override: Option<&str>,
) -> Option<String> {
    if explicit_profile.is_some() {
        return explicit_profile;
    }
    let config = VmConfig::load(config_path).ok()?;
    AppConfig::resolve_profile_name(&config, None, provider_override)
}

fn resolve_noninteractive(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    environment: Option<String>,
) -> ResolvedEnvironment {
    match environment.as_deref() {
        Some("mac") => {
            let provider = EnvironmentKind::Mac.default_provider();
            let profile = selected_profile(config_path.clone(), profile, Some(provider))
                .or_else(|| mac_profile(config_path));
            ResolvedEnvironment::new(Some(provider.to_string()), profile, Some("mac".to_string()))
        }
        Some("linux") | Some("container") => {
            let provider = EnvironmentKind::Linux.default_provider();
            ResolvedEnvironment::new(
                Some(provider.to_string()),
                selected_profile(config_path, profile, Some(provider)),
                None,
            )
        }
        Some(environment) => {
            if profile.is_none() && profile_exists(config_path.clone(), environment) {
                return ResolvedEnvironment::new(
                    None,
                    Some(environment.to_string()),
                    target_for_profile(config_path, environment),
                );
            }

            let profile = selected_profile(config_path, profile, None);
            ResolvedEnvironment::new(None, profile, Some(environment.to_string()))
        }
        None => {
            let profile = selected_profile(config_path.clone(), profile, None);
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
    profile_is_macos(Some(profile_config)).then(|| "mac".to_string())
}

pub(super) fn resolve_environment(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    environment: Option<String>,
) -> VmResult<ResolvedEnvironment> {
    if environment.is_some() || profile.is_some() {
        return Ok(resolve_noninteractive(config_path, profile, environment));
    }

    let config = VmConfig::load(config_path.clone()).map_err(VmError::from)?;
    if AppConfig::resolve_profile_name(&config, None, None).is_some() {
        return Ok(resolve_noninteractive(config_path, None, None));
    }

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
            (
                name.clone(),
                format!("{} ({name} profile)", profile_label(profile_config)),
                profile_is_macos(Some(profile_config)).then(|| "mac".to_string()),
            )
        })
        .collect();

    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        let names = choices
            .iter()
            .map(|(name, _, _)| name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(VmError::validation(
            "Multiple configuration profiles are available",
            Some(format!("Use --profile with one of: {names}")),
        ));
    }

    let labels: Vec<&str> = choices.iter().map(|(_, label, _)| label.as_str()).collect();
    let selected = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Which environment?")
        .items(&labels)
        .default(0)
        .interact()
        .map_err(|error| VmError::general(error, "Failed to read environment selection"))?;

    Ok(ResolvedEnvironment {
        provider_override: None,
        profile: Some(choices[selected].0.clone()),
        target: choices[selected].2.clone(),
    })
}

fn profile_label(profile: &VmConfig) -> &'static str {
    match profile.provider.as_deref() {
        Some("docker") | Some("podman") => "Container",
        Some("tart") if profile_is_macos(Some(profile)) => "macOS",
        Some("tart") => "Linux",
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
    use super::{resolve_environment, resolve_noninteractive, ResolvedEnvironment};
    use std::io::IsTerminal;
    use std::path::PathBuf;

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

    fn write_config(name: &str, contents: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("vm-environment-{name}-{}.yaml", std::process::id()));
        std::fs::write(&path, contents).unwrap();
        path
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
    fn configured_default_profile_does_not_prompt() {
        let path = write_config(
            "default",
            r#"
default_profile: docker
profiles:
  docker:
    provider: docker
  tart:
    provider: tart
"#,
        );

        let resolved = resolve_environment(Some(path.clone()), None, None).unwrap();
        assert_resolved(resolved, None, Some("docker"), None);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn noninteractive_ambiguity_lists_profiles() {
        if std::io::stdin().is_terminal() && std::io::stderr().is_terminal() {
            return;
        }
        let path = write_config(
            "ambiguous",
            r#"
profiles:
  docker:
    provider: docker
  tart:
    provider: tart
"#,
        );

        let error = resolve_environment(Some(path.clone()), None, None).unwrap_err();
        assert_eq!(
            error.hint(),
            Some("Use --profile with one of: docker, tart")
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn resolver_targets_mac_instance_for_macos_profile() {
        let path = write_config(
            "macos",
            r#"
profiles:
  tart:
    provider: tart
    tart:
      guest_os: macos
"#,
        );

        assert_resolved(
            resolve_noninteractive(Some(path.clone()), Some("tart".into()), None),
            None,
            Some("tart"),
            Some("mac"),
        );
        assert_resolved(
            resolve_noninteractive(Some(path.clone()), None, Some("tart".into())),
            None,
            Some("tart"),
            Some("mac"),
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn resolver_does_not_target_instance_for_container_profile() {
        let path = write_config(
            "container",
            r#"
profiles:
  docker:
    provider: docker
"#,
        );

        assert_resolved(
            resolve_noninteractive(Some(path.clone()), Some("docker".into()), None),
            None,
            Some("docker"),
            None,
        );
        assert_resolved(
            resolve_noninteractive(Some(path.clone()), None, Some("docker".into())),
            None,
            Some("docker"),
            None,
        );
        std::fs::remove_file(path).unwrap();
    }
}
