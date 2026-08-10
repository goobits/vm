use super::provider::tart_run_log_path;
use crate::tart_base;
use duct::cmd;
use tracing::info;
use vm_config::config::{BoxSpec, VmConfig};
use vm_core::error::{Result, VmError};

mod ai_tools;
mod home_state;
mod host;
mod packages;
mod services;
mod shell_config;

pub struct TartProvisioner {
    instance_name: String,
    project_dir: String,
    tart_home: Option<String>,
}

impl TartProvisioner {
    pub fn new(instance_name: String, project_dir: String, tart_home: Option<String>) -> Self {
        Self {
            instance_name,
            project_dir,
            tart_home,
        }
    }

    fn host_shell(&self, command: &str) -> duct::Expression {
        let mut expr = cmd!("sh", "-c", command);
        if let Some(tart_home) = &self.tart_home {
            expr = expr.env("TART_HOME", tart_home);
        }
        expr
    }

    /// Run provisioning scripts over SSH
    pub fn provision(&self, config: &VmConfig) -> Result<()> {
        info!("Starting Tart VM provisioning for {}", self.instance_name);

        // 1. Wait for VM to be ready
        self.wait_for_ssh()?;

        // 2. Ensure workspace share is mounted
        self.ensure_workspace_mount()?;
        self.repair_home_state()?;

        // 3. Apply host sync behaviors that Tart can support cleanly
        self.sync_dotfiles(config)?;
        self.sync_ssh_config(config)?;

        // 4. Apply runtime configuration that should behave the same across providers
        self.apply_git_config(config)?;
        self.apply_canonical_shell_config(config)?;
        self.apply_shell_overrides(config)?;

        // 5. Honor generic package lists from vm.yaml before framework-specific setup
        self.provision_generic_packages(config)?;
        self.provision_ai_tools(config)?;
        self.install_docker_if_requested(config)?;
        // Mount AI config after CLI installation so installers do not write into
        // host-synced config directories such as ~/.claude.
        self.ensure_host_sync_mounts(config)?;

        // 6. Detect framework and install dependencies
        self.provision_framework_dependencies(config)?;

        // 7. Run custom provision scripts if present
        self.run_custom_provision_scripts(config)?;

        // 8. Start services
        self.start_services(config)?;

        info!("Provisioning completed successfully");
        Ok(())
    }

    fn wait_for_ssh(&self) -> Result<()> {
        use std::thread;
        use std::time::Duration;

        info!("Waiting for SSH to be ready...");

        let log_path = tart_run_log_path(&self.instance_name);
        for _attempt in 1..=30 {
            let mut expr = cmd!("tart", "exec", &self.instance_name, "echo", "ready");
            if let Some(tart_home) = &self.tart_home {
                expr = expr.env("TART_HOME", tart_home);
            }
            let result = expr.stderr_null().stdout_null().run();

            if result.is_ok() {
                info!("SSH is ready");
                return Ok(());
            }

            if let Ok(log) = std::fs::read_to_string(&log_path) {
                if log.contains("The number of VMs exceeds the system limit") {
                    return Err(VmError::Provider(format!(
                        "Tart could not start because the host VM limit was reached. Stop another Tart VM and retry. Tart run log: {}{}",
                        log_path,
                        self.read_host_log_tail(&log_path, 40)
                    )));
                }
            }

            thread::sleep(Duration::from_secs(2));
        }

        let log_tail = self.read_host_log_tail(&log_path, 40);

        Err(VmError::Provider(format!(
            "SSH not ready after 60 seconds. Tart run log: {}{}",
            log_path, log_tail
        )))
    }

    fn read_host_log_tail(&self, log_path: &str, max_lines: usize) -> String {
        let Ok(content) = std::fs::read_to_string(log_path) else {
            return String::new();
        };

        let tail = content
            .lines()
            .rev()
            .take(max_lines)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");

        if tail.trim().is_empty() {
            String::new()
        } else {
            format!("\nLast {} log lines:\n{}", max_lines, tail)
        }
    }

    fn shell_escape_single_quotes(input: &str) -> String {
        input.replace('\'', "'\"'\"'")
    }

    fn shell_quote_packages(packages: &[String]) -> String {
        packages
            .iter()
            .map(|package| format!("'{}'", Self::shell_escape_single_quotes(package)))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn virtiofs_mount_command(tag: &str, target: &str) -> String {
        let tag = Self::shell_escape_single_quotes(tag);
        let target = Self::shell_escape_single_quotes(target);
        format!(
            r#"is_mounted() {{
  if [ -x /sbin/mount ]; then
    /sbin/mount | grep -F "on $1 " >/dev/null 2>&1
  elif command -v mount >/dev/null 2>&1; then
    mount | grep -F "on $1 " >/dev/null 2>&1
  else
    return 1
  fi
}}
target='{target}';
if [ -x /sbin/mount_virtiofs ]; then
  mkdir -p "$target"
  if ! is_mounted "$target"; then
    /sbin/mount_virtiofs '{tag}' "$target"
  fi
else
  if ! is_mounted "$target"; then
    if command -v sudo >/dev/null 2>&1; then SUDO=sudo; else SUDO=""; fi
    $SUDO mkdir -p "$target" && $SUDO mount -t virtiofs '{tag}' "$target"
  fi
fi"#
        )
    }

    pub(crate) fn ensure_workspace_mount(&self) -> Result<()> {
        self.ssh_exec(&Self::virtiofs_mount_command(
            "workspace",
            &self.project_dir,
        ))
        .map(|_| ())
    }

    fn ssh_exec(&self, command: &str) -> Result<String> {
        let mut expr = cmd!("tart", "exec", &self.instance_name, "bash", "-c", command);
        if let Some(tart_home) = &self.tart_home {
            expr = expr.env("TART_HOME", tart_home);
        }
        let output = expr
            .read()
            .map_err(|e| VmError::Provider(format!("Exec command failed: {}", e)))?;

        Ok(output)
    }

    fn is_macos_guest(&self, config: &VmConfig) -> bool {
        Self::guest_os(config) == "macos"
    }

    fn guest_os(config: &VmConfig) -> &'static str {
        if matches!(config.os.as_deref(), Some("macos")) {
            return "macos";
        }

        if matches!(config.os.as_deref(), Some("linux")) {
            return "linux";
        }

        if matches!(
            config.tart.as_ref().and_then(|t| t.guest_os.as_deref()),
            Some("macos")
        ) {
            return "macos";
        }

        if matches!(
            config.tart.as_ref().and_then(|t| t.guest_os.as_deref()),
            Some("linux")
        ) {
            return "linux";
        }

        if let Some(BoxSpec::String(name)) = config.vm.as_ref().and_then(|vm| vm.get_box_spec()) {
            if let Some(guest_os) = tart_base::guest_os(&name) {
                return guest_os;
            }
            if name.contains("ubuntu") || name.contains("debian") || name.contains("linux") {
                return "linux";
            }
        }

        if let Some(image) = config.tart.as_ref().and_then(|t| t.image.as_deref()) {
            if image.contains("macos") {
                return "macos";
            }
            if image.contains("ubuntu") || image.contains("debian") || image.contains("linux") {
                return "linux";
            }
        }

        "macos"
    }

    fn ensure_homebrew(&self) -> Result<()> {
        self.ssh_exec(
            r#"if [ -x /opt/homebrew/bin/brew ]; then
  eval "$(/opt/homebrew/bin/brew shellenv)"
fi
if ! command -v brew >/dev/null 2>&1; then
  echo "Homebrew is required for macOS Tart provisioning" >&2
  exit 1
fi"#,
        )?;
        Ok(())
    }

    fn user_bin_path(config: &VmConfig) -> &'static str {
        if Self::guest_os(config) == "macos" {
            "/opt/homebrew/bin:$HOME/.local/bin:$PATH"
        } else {
            "$HOME/.local/bin:$PATH"
        }
    }

    fn macos_brew_preamble() -> &'static str {
        r#"if [ -x /opt/homebrew/bin/brew ]; then
  eval "$(/opt/homebrew/bin/brew shellenv)"
fi"#
    }
}

#[cfg(test)]
mod tests {
    use super::TartProvisioner;
    use indexmap::IndexMap;
    use vm_config::config::{BoxSpec, ProjectConfig, TerminalConfig, VmConfig, VmSettings};

    #[test]
    fn host_shell_applies_tart_home() {
        let provisioner = TartProvisioner::new(
            "vm-mac".to_string(),
            "/workspace".to_string(),
            Some("/Volumes/External SSD/Tart".to_string()),
        );

        let output = provisioner
            .host_shell("printf '%s' \"$TART_HOME\"")
            .read()
            .unwrap();

        assert_eq!(output, "/Volumes/External SSD/Tart");
    }

    #[test]
    fn render_shell_overrides_includes_environment_exports() {
        let mut config = VmConfig::default();
        config
            .environment
            .insert("EDITOR".to_string(), "nvim".to_string());

        let rendered = TartProvisioner::render_shell_overrides(&config).unwrap();

        assert!(rendered.contains("export EDITOR='nvim'"));
    }

    #[test]
    fn render_shell_overrides_returns_none_when_empty() {
        let config = VmConfig {
            aliases: IndexMap::new(),
            environment: IndexMap::new(),
            ..Default::default()
        };

        let rendered = TartProvisioner::render_shell_overrides(&config);
        assert!(rendered.is_none());
    }

    #[test]
    fn package_names_are_shell_quoted() {
        let packages = vec![
            "safe".to_string(),
            "pkg; touch /tmp/injected".to_string(),
            "it's".to_string(),
        ];

        assert_eq!(
            TartProvisioner::shell_quote_packages(&packages),
            "'safe' 'pkg; touch /tmp/injected' 'it'\"'\"'s'"
        );
    }

    #[test]
    fn virtiofs_mount_values_are_shell_quoted() {
        let command = TartProvisioner::virtiofs_mount_command("tag's", "/path with 'quotes'");

        assert!(command.contains("mount_virtiofs 'tag'\"'\"'s'"));
        assert!(command.contains("target='/path with '\"'\"'quotes'\"'\"'';"));
    }

    #[test]
    fn guest_os_detects_vibe_tart_base_as_macos() {
        let config = VmConfig {
            vm: Some(VmSettings {
                r#box: Some(BoxSpec::String("vibe-tart-sequoia-base".to_string())),
                ..Default::default()
            }),
            ..Default::default()
        };

        assert_eq!(TartProvisioner::guest_os(&config), "macos");
    }

    #[test]
    fn guest_os_detects_linux_base_name() {
        let config = VmConfig {
            vm: Some(VmSettings {
                r#box: Some(BoxSpec::String("vibe-tart-linux-base".to_string())),
                ..Default::default()
            }),
            ..Default::default()
        };

        assert_eq!(TartProvisioner::guest_os(&config), "linux");
    }

    #[test]
    fn guest_os_respects_explicit_config_os() {
        let config = VmConfig {
            os: Some("macos".to_string()),
            vm: Some(VmSettings {
                r#box: Some(BoxSpec::String("custom-base".to_string())),
                ..Default::default()
            }),
            ..Default::default()
        };

        assert_eq!(TartProvisioner::guest_os(&config), "macos");
    }

    #[test]
    fn guest_os_defaults_ambiguous_custom_tart_base_to_macos() {
        let config = VmConfig {
            vm: Some(VmSettings {
                r#box: Some(BoxSpec::String("custom-team-base".to_string())),
                ..Default::default()
            }),
            tart: Some(vm_config::config::TartConfig {
                image: Some("ghcr.io/example/custom-base:latest".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        assert_eq!(TartProvisioner::guest_os(&config), "macos");
    }

    #[test]
    fn canonical_zshrc_renders_for_macos_tart() {
        let mut config = VmConfig {
            project: Some(ProjectConfig {
                name: Some("vm".to_string()),
                ..Default::default()
            }),
            vm: Some(VmSettings {
                r#box: Some(BoxSpec::String("vibe-tart-sequoia-base".to_string())),
                ..Default::default()
            }),
            terminal: Some(TerminalConfig {
                username: Some("vm-dev".to_string()),
                theme: Some("dracula".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        config
            .aliases
            .insert("gs".to_string(), "git status".to_string());

        let rendered = TartProvisioner::render_canonical_zshrc(&config, "/workspace").unwrap();

        assert!(rendered.contains("PROMPT='🍎 "));
        assert!(rendered.contains("alias gs='git status'"));
        assert!(rendered.contains("VM_SHELL_CONFIG_VERSION=4"));
        assert!(rendered.contains("yocodex()"));
        assert!(rendered.contains("vm_repair_codex_state"));
        assert!(rendered.contains("VM_PROJECT_PATH='/workspace'"));
    }
}
