use crate::error::{VmError, VmResult};
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
    let profile = selected_profile(config_path.clone(), profile, None);
    ResolvedEnvironment::new(None, profile, environment)
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

    let mut choices = profiles.keys().cloned().collect::<Vec<_>>();
    choices.sort();
    let names = choices.join(", ");
    Err(VmError::validation(
        "Multiple configuration profiles are available",
        Some(format!("Use --profile with one of: {names}")),
    ))
}

#[cfg(test)]
mod tests {
    use super::{resolve_environment, resolve_noninteractive, ResolvedEnvironment};
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
    fn resolver_treats_kind_words_as_names() {
        let missing_config =
            Some(std::env::temp_dir().join("vm-missing-config-for-shell-test.yaml"));
        assert_resolved(
            resolve_noninteractive(missing_config, None, Some("mac".into())),
            None,
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
    fn default_shell_preserves_the_configured_provider() {
        let path = write_config(
            "configured-provider",
            r#"
provider: tart
tart:
  guest_os: linux
"#,
        );

        let resolved = resolve_environment(Some(path.clone()), None, None).unwrap();
        assert_resolved(resolved, None, None, None);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn noninteractive_ambiguity_lists_profiles() {
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
    fn resolver_keeps_profile_separate_from_environment() {
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
            None,
        );
        assert_resolved(
            resolve_noninteractive(Some(path.clone()), None, Some("tart".into())),
            None,
            Some("tart"),
            Some("tart"),
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
            Some("docker"),
        );
        std::fs::remove_file(path).unwrap();
    }
}
