use vm_config::config::VmConfig;
use vm_packages::ClientEnvironment;
use vm_provider::{CommandProvider, Provider};

use crate::error::{VmError, VmResult};

use super::access::configured_client_environment;

const INSTALL_GUEST_CLIENT: &str = r#"set -eu
. /etc/profile.d/vm-packages.sh
url=${VM_PACKAGES_CLIENT_URL:?managed guest client URL unavailable}
token=${CARGO_REGISTRIES_VM_TOKEN:?package read token unavailable}
destination="$HOME/.local/bin/vm"
state="$HOME/.local/state/vm"
mkdir -p "$(dirname "$destination")" "$state"
task=$(mktemp -d "$state/client.XXXXXX")
cleanup() {
  rm -rf "$task"
}
trap cleanup EXIT HUP INT TERM
curl_args="--fail --silent --show-error --location --connect-timeout 5 --max-time 600 --retry 2"
printf 'header = "Authorization: Bearer %s"\n' "$token" > "$task/curl.conf"
chmod 0600 "$task/curl.conf"
if ! curl $curl_args --config "$task/curl.conf" \
    --output "$task/digest" "$url.sha256"; then
  if test -x "$destination"; then
    exit 0
  fi
  echo "Managed guest VM client is unavailable; run 'vm packages up' on the controller host" >&2
  exit 1
fi
expected=$(tr -d '[:space:]' < "$task/digest")
case "$expected" in
  *[!0-9a-f]*|'') echo "Managed guest VM client digest is invalid" >&2; exit 1 ;;
esac
test "${#expected}" -eq 64 || {
  echo "Managed guest VM client digest is invalid" >&2
  exit 1
}
installed=$(cat "$state/client.sha256" 2>/dev/null || true)
if test -x "$destination" && test "$installed" = "$expected"; then
  exit 0
fi
curl $curl_args --config "$task/curl.conf" \
  --output "$task/vm" "$url"
if command -v sha256sum >/dev/null 2>&1; then
  printf '%s  %s\n' "$expected" "$task/vm" | sha256sum -c - >/dev/null
elif command -v shasum >/dev/null 2>&1; then
  actual=$(shasum -a 256 "$task/vm" | awk '{print $1}')
  test "$actual" = "$expected"
else
  echo "No SHA-256 verifier is installed" >&2
  exit 1
fi
chmod 0755 "$task/vm"
"$task/vm" --version >/dev/null
mv -f "$task/vm" "$destination"
printf '%s\n' "$expected" > "$task/client.sha256"
mv -f "$task/client.sha256" "$state/client.sha256"
"#;

const RECONCILE_GIT_SETTING: &str = r#"current=$(git config --global --get "$1" 2>/dev/null || true)
if [ "$current" != "$2" ]; then
  git config --global "$1" "$2"
fi"#;

pub(in crate::commands) fn apply_client_environment(config: &mut VmConfig) -> VmResult<()> {
    let Some((client, edge)) = configured_client_environment(config)? else {
        return Ok(());
    };
    config.package_edge = Some(edge);
    apply_environment(config, &client);
    Ok(())
}

pub(in crate::commands) fn reconcile_client_settings(
    provider: &dyn Provider,
    environment: &str,
    config: &VmConfig,
) -> VmResult<()> {
    if !config.environment.contains_key("VM_PACKAGES_AGENT_TOKEN") {
        return Ok(());
    }
    let mut effective_config = config.clone();
    if let Ok(instances) = provider.list_instances() {
        if let Some(project) = instances
            .iter()
            .find(|instance| instance.name == environment)
            .and_then(|instance| instance.project.as_deref())
        {
            effective_config
                .project
                .get_or_insert_with(Default::default)
                .name = Some(project.to_string());
        }
    }
    let Some((client, _)) = configured_client_environment(&effective_config)? else {
        return Ok(());
    };
    crate::commands::managed_guest::install_package_settings(
        provider,
        environment,
        &client.managed_settings(),
    )?;
    reconcile_git_identity(provider, environment, &effective_config)?;
    provider
        .exec(
            Some(environment),
            &[
                "/bin/sh".to_string(),
                "-c".to_string(),
                INSTALL_GUEST_CLIENT.to_string(),
            ],
        )
        .map_err(VmError::from)
}

fn reconcile_git_identity(
    provider: &dyn CommandProvider,
    environment: &str,
    config: &VmConfig,
) -> VmResult<()> {
    for (key, value) in configured_git_identity(config)? {
        provider
            .exec(
                Some(environment),
                &[
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    RECONCILE_GIT_SETTING.to_string(),
                    "vm-git-identity".to_string(),
                    key.to_string(),
                    value,
                ],
            )
            .map_err(VmError::from)?;
    }
    Ok(())
}

fn configured_git_identity(config: &VmConfig) -> VmResult<Vec<(&'static str, String)>> {
    if config
        .host_sync
        .as_ref()
        .is_some_and(|host_sync| !host_sync.git_config)
    {
        return Ok(Vec::new());
    }
    let Some(git) = &config.git_config else {
        return Ok(Vec::new());
    };
    let mut settings = Vec::new();
    for (key, value) in [
        ("user.name", git.user_name.as_deref()),
        ("user.email", git.user_email.as_deref()),
    ] {
        let Some(value) = value else {
            continue;
        };
        if value.trim().is_empty() || value.contains(['\0', '\n', '\r']) {
            return Err(VmError::validation(
                format!("Host Git {key} must be one non-empty line"),
                Some("Fix the host Git configuration and rerun `vm tools update`"),
            ));
        }
        settings.push((key, value.to_string()));
    }
    Ok(settings)
}

fn apply_environment(config: &mut VmConfig, client: &ClientEnvironment) {
    config.environment.extend(client.variables());
}

#[cfg(test)]
mod tests {
    use super::{
        apply_environment, configured_git_identity, INSTALL_GUEST_CLIENT, RECONCILE_GIT_SETTING,
    };
    use vm_config::config::{HostSyncConfig, VmConfig};
    use vm_config::detector::git::GitConfig;
    use vm_packages::{ClientEnvironment, RegistryEndpoints};

    #[test]
    fn managed_package_settings_replace_project_redirects() {
        let endpoints = RegistryEndpoints::new("http://packages.internal:3080").unwrap();
        let client = ClientEnvironment::new(endpoints, "read-token").unwrap();
        let mut config = VmConfig::default();
        config.environment.insert(
            "NPM_CONFIG_REGISTRY".into(),
            "https://untrusted.invalid".into(),
        );

        apply_environment(&mut config, &client);

        assert!(config.environment["NPM_CONFIG_REGISTRY"].contains("packages.internal"));
        assert_eq!(
            config.environment["CARGO_SOURCE_CRATES_IO_REPLACE_WITH"],
            "vm"
        );
    }

    #[test]
    fn guest_client_repair_is_authenticated_verified_and_atomic() {
        assert!(INSTALL_GUEST_CLIENT.contains("VM_PACKAGES_CLIENT_URL"));
        assert!(INSTALL_GUEST_CLIENT.contains("--config \"$task/curl.conf\""));
        assert!(INSTALL_GUEST_CLIENT.contains("sha256sum -c"));
        assert!(INSTALL_GUEST_CLIENT.contains("mv -f \"$task/vm\" \"$destination\""));
        assert!(!INSTALL_GUEST_CLIENT.contains("--header \"Authorization"));
    }

    #[test]
    fn git_identity_reconciliation_is_idempotent_and_honors_host_sync() {
        let mut config = VmConfig {
            git_config: Some(GitConfig {
                user_name: Some("Agent User".into()),
                user_email: Some("agent@example.test".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            configured_git_identity(&config).unwrap(),
            [
                ("user.name", "Agent User".to_string()),
                ("user.email", "agent@example.test".to_string())
            ]
        );
        assert!(RECONCILE_GIT_SETTING.contains("current=$(git config --global --get"));
        assert!(RECONCILE_GIT_SETTING.contains("if [ \"$current\" != \"$2\" ]"));

        config.host_sync = Some(HostSyncConfig {
            git_config: false,
            ..Default::default()
        });
        assert!(configured_git_identity(&config).unwrap().is_empty());
    }

    #[test]
    fn git_identity_rejects_multiline_values() {
        let config = VmConfig {
            git_config: Some(GitConfig {
                user_name: Some("Agent\nInjected".into()),
                user_email: Some("agent@example.test".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(configured_git_identity(&config).is_err());
    }
}
