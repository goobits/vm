use super::{provider::tart_run_log_path, ssh_identity::TartSshIdentity, TartCommand};
use crate::{
    project_plan::ProjectPlan,
    shell_session::{quote_posix_argument, quote_posix_home_path},
    tart_base,
};
use duct::cmd;
use tracing::info;
use vm_config::config::{BoxSpec, MountAccess, VmConfig};
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
    command: TartCommand,
}

pub(super) type GuestCommand = (&'static str, String);

impl TartProvisioner {
    pub fn new(instance_name: String, project_dir: String, command: TartCommand) -> Self {
        Self {
            instance_name,
            project_dir,
            command,
        }
    }

    fn host_shell(&self, command: &str) -> duct::Expression {
        self.command.with_env(cmd!("sh", "-c", command))
    }

    /// Run provisioning scripts over SSH
    pub(crate) fn provision(&self, config: &VmConfig, project_plan: &ProjectPlan) -> Result<()> {
        info!("Starting Tart VM provisioning for {}", self.instance_name);

        // 1. Wait for VM to be ready
        self.wait_for_ssh()?;

        // Seed the VM-owned host identity while the guest-agent transport is
        // available. Existing guests can bootstrap this once over SSH.
        let identity = TartSshIdentity::ensure()?;
        self.ssh_exec(&identity.authorized_key_script())?;

        // 2. Mount the workspace and repair guest state in one SSH batch.
        self.ssh_exec_batch(vec![
            ("workspace mount", self.workspace_mount_command(config)),
            ("guest home repair", self.home_state_repair_command()),
        ])?;

        // 3. Apply host sync behaviors that Tart can support cleanly
        self.sync_dotfiles(config)?;
        self.sync_ssh_config(config)?;

        // 4. Apply runtime configuration and install software in one SSH batch.
        let mut setup = self.guest_configuration_commands(config)?;
        setup.extend(self.guest_software_commands(config, project_plan));
        self.ssh_exec_batch(setup)?;
        self.sync_codex_runtime_config(config)?;
        // Mount AI config after CLI installation so installers do not write into
        // host-synced config directories such as ~/.claude.
        let mut finalization = Vec::new();
        if let Some(mounts) = self.host_sync_mount_command(config) {
            finalization.push(("host sync mounts", mounts));
        }
        finalization.push((
            "custom project provisioning",
            self.custom_provision_command(),
        ));
        self.ssh_exec_batch(finalization)?;

        info!("Provisioning completed successfully");
        Ok(())
    }

    fn wait_for_ssh(&self) -> Result<()> {
        use std::thread;
        use std::time::Duration;

        info!("Waiting for SSH to be ready...");

        let log_path = tart_run_log_path(&self.instance_name);
        for _attempt in 1..=30 {
            let result = self
                .command
                .expr(&["exec", &self.instance_name, "echo", "ready"])
                .stderr_null()
                .stdout_null()
                .run();

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

    fn shell_quote_packages(packages: &[String]) -> String {
        packages
            .iter()
            .map(|package| quote_posix_argument(package))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub(super) fn virtiofs_mount_command(tag: &str, target: &str) -> String {
        let tag = quote_posix_argument(tag);
        let target = quote_posix_home_path(target);
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
target={target};
if command -v sudo >/dev/null 2>&1 && sudo -n true >/dev/null 2>&1; then
  SUDO="sudo -n"
else
  SUDO=""
fi
if [ -x /sbin/mount_virtiofs ]; then
  $SUDO mkdir -p "$target"
  if ! is_mounted "$target"; then
    $SUDO /sbin/mount_virtiofs {tag} "$target"
  fi
else
  if ! is_mounted "$target"; then
    $SUDO mkdir -p "$target" && $SUDO mount -t virtiofs {tag} "$target"
  fi
fi"#
        )
    }

    pub(super) fn workspace_mount_command(&self, config: &VmConfig) -> String {
        let read_only = config
            .project
            .as_ref()
            .is_some_and(|project| project.workspace_access == MountAccess::ReadOnly);
        if !read_only || self.is_macos_guest(config) {
            return Self::virtiofs_mount_command("workspace", &self.project_dir);
        }

        let source = "/mnt/vm-workspace-source";
        let source_mount = Self::virtiofs_mount_command("workspace", source);
        let target = quote_posix_argument(&self.project_dir);
        format!(
            r#"{source_mount}
target={target}
state="$HOME/.local/share/vm/workspace-overlay"
mkdir -p "$state/upper" "$state/work"
if ! is_mounted "$target"; then
  if command -v sudo >/dev/null 2>&1; then SUDO=sudo; else SUDO=""; fi
  $SUDO mkdir -p "$target"
  $SUDO mount -t overlay vm-workspace \
    -o "lowerdir={source},upperdir=$state/upper,workdir=$state/work" "$target"
fi
if command -v sudo >/dev/null 2>&1; then SUDO=sudo; else SUDO=""; fi
$SUDO mkdir -p "$target/node_modules"
$SUDO chown "$(id -u):$(id -g)" "$target/node_modules"
find {source} -mindepth 1 -maxdepth 1 ! -name node_modules -print0 |
while IFS= read -r -d '' entry; do
  destination="$target/${{entry##*/}}"
  if ! is_mounted "$destination"; then
    $SUDO mount --bind "$entry" "$destination"
    $SUDO mount -o remount,bind,ro "$destination"
  fi
done
$SUDO chmod 0555 "$target""#
        )
    }

    pub(crate) fn ensure_workspace_mount(&self, config: &VmConfig) -> Result<()> {
        self.ssh_exec(&self.workspace_mount_command(config))
            .map(|_| ())
    }

    fn ssh_exec(&self, command: &str) -> Result<String> {
        let output = self
            .command
            .expr(&["exec", &self.instance_name, "bash", "-c", command])
            .read()
            .map_err(|e| VmError::Provider(format!("Exec command failed: {}", e)))?;

        Ok(output)
    }

    fn render_command_batch(commands: &[GuestCommand]) -> Option<String> {
        let commands = commands
            .iter()
            .filter(|(_, command)| !command.trim().is_empty())
            .map(|(label, command)| {
                format!(
                    "printf '%s\\n' 'VM_PROVISION_STEP={label}' >&2\n(\n[ ! -r \"$HOME/.vm_runtime_env\" ] || . \"$HOME/.vm_runtime_env\"\n[ ! -r \"$HOME/.vm_shell_overrides\" ] || . \"$HOME/.vm_shell_overrides\"\n{}\n)",
                    command.trim()
                )
            })
            .collect::<Vec<_>>();

        (!commands.is_empty()).then(|| format!("set -eo pipefail\n{}", commands.join("\n")))
    }

    fn ssh_exec_batch(&self, commands: Vec<GuestCommand>) -> Result<()> {
        let Some(command) = Self::render_command_batch(&commands) else {
            return Ok(());
        };
        self.ssh_exec(&command).map(|_| ())
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

        "macos"
    }

    fn user_bin_path(config: &VmConfig) -> &'static str {
        if Self::guest_os(config) == "macos" {
            "/opt/homebrew/bin:$HOME/.local/bin:$PATH"
        } else {
            "$HOME/.local/bin:$PATH"
        }
    }

    fn homebrew_preamble() -> &'static str {
        r#"if [ -x /opt/homebrew/bin/brew ]; then
  eval "$(/opt/homebrew/bin/brew shellenv)"
fi
if ! command -v brew >/dev/null 2>&1; then
  echo "Homebrew is required for macOS Tart provisioning" >&2
  exit 1
fi"#
    }
}

#[cfg(test)]
mod tests {
    use super::TartProvisioner;
    use crate::project_plan::ProjectPlan;
    use indexmap::IndexMap;
    use std::path::PathBuf;
    #[cfg(unix)]
    use std::process::Command;
    use vm_config::config::{
        BoxSpec, ProjectConfig, ServiceConfig, TartConfig, TerminalConfig, VmConfig, VmSettings,
    };

    #[test]
    fn host_shell_applies_tart_home() {
        let provisioner = TartProvisioner::new(
            "vm-mac".to_string(),
            "/workspace".to_string(),
            crate::tart::TartCommand::new(Some(PathBuf::from("/Volumes/External SSD/Tart"))),
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
    fn shell_configuration_has_one_owner_and_removes_stale_overrides() {
        let config = VmConfig::default();

        let command = TartProvisioner::shell_config_command(&config, "/workspace").unwrap();

        assert_eq!(command.matches("touch \"$HOME/.bashrc\"").count(), 1);
        assert!(command.contains("rm -f \"$HOME/.vm_shell_overrides\""));
        assert!(command.contains("VM_SHELL_CONFIG_VERSION=6"));
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

    #[cfg(unix)]
    #[test]
    fn guest_command_batches_are_labeled_and_fail_fast() {
        let batch = TartProvisioner::render_command_batch(&[
            ("failing step", "exit 7".to_string()),
            ("skipped step", "printf should-not-run".to_string()),
        ])
        .unwrap();
        let output = Command::new("/bin/bash")
            .args(["-c", &batch])
            .output()
            .unwrap();

        assert_eq!(output.status.code(), Some(7));
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("VM_PROVISION_STEP=failing step"));
        assert!(!stderr.contains("VM_PROVISION_STEP=skipped step"));
    }

    #[test]
    fn linux_databases_share_one_package_transaction() {
        let provisioner = TartProvisioner::new(
            "vm-linux".to_string(),
            "/workspace".to_string(),
            crate::tart::TartCommand::new(None),
        );
        let mut config = VmConfig {
            os: Some("linux".to_string()),
            ..Default::default()
        };
        for service in ["postgresql", "redis"] {
            config.services.insert(
                service.to_string(),
                ServiceConfig {
                    enabled: true,
                    ..Default::default()
                },
            );
        }

        let command = provisioner.database_command(&config).unwrap();

        assert_eq!(command.matches("apt-get update").count(), 1);
        assert!(command.contains("postgresql postgresql-contrib redis-server"));
        assert!(command.contains("systemctl enable --now postgresql"));
        assert!(command.contains("systemctl enable --now redis-server"));
    }

    #[test]
    fn tart_setup_uses_one_ordered_guest_batch() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("package.json"), "{}\n").unwrap();
        std::fs::write(project.path().join("package-lock.json"), "{}\n").unwrap();
        let provisioner = TartProvisioner::new(
            "vm-linux".to_string(),
            "/workspace".to_string(),
            crate::tart::TartCommand::new(None),
        );
        let mut config = VmConfig {
            os: Some("linux".to_string()),
            tart: Some(TartConfig {
                install_docker: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        config.services.insert(
            "postgresql".to_string(),
            ServiceConfig {
                enabled: true,
                ..Default::default()
            },
        );
        let plan = ProjectPlan::detect(project.path(), &config);

        let mut commands = provisioner.guest_configuration_commands(&config).unwrap();
        commands.extend(provisioner.guest_software_commands(&config, &plan));
        let labels = commands.iter().map(|(label, _)| *label).collect::<Vec<_>>();

        assert_eq!(
            labels,
            [
                "shell configuration",
                "Docker runtime",
                "Node.js toolchain",
                "Node.js project dependencies",
                "database services",
            ]
        );
        let batch = TartProvisioner::render_command_batch(&commands).unwrap();
        assert_eq!(batch.matches("VM_PROVISION_STEP=").count(), labels.len());
        #[cfg(unix)]
        {
            assert!(Command::new("/bin/bash")
                .args(["-n", "-c", &batch])
                .status()
                .unwrap()
                .success());

            config.os = Some("macos".to_string());
            let mut macos_commands = provisioner.guest_configuration_commands(&config).unwrap();
            macos_commands.extend(provisioner.guest_software_commands(&config, &plan));
            let macos_batch = TartProvisioner::render_command_batch(&macos_commands).unwrap();
            assert!(Command::new("/bin/bash")
                .args(["-n", "-c", &macos_batch])
                .status()
                .unwrap()
                .success());
        }
    }

    #[test]
    fn custom_provision_paths_are_shell_quoted() {
        let provisioner = TartProvisioner::new(
            "vm-linux".to_string(),
            "/work/it's here".to_string(),
            crate::tart::TartCommand::new(None),
        );

        let command = provisioner.custom_provision_command();

        assert!(command.contains("'/work/it'\"'\"'s here'/provision.sh"));
        assert!(command.contains("cd '/work/it'\"'\"'s here'"));
    }

    #[test]
    fn virtiofs_mount_values_are_shell_quoted() {
        let command = TartProvisioner::virtiofs_mount_command("tag's", "/path with 'quotes'");

        assert!(command.contains("mount_virtiofs 'tag'\"'\"'s'"));
        assert!(command.contains("target='/path with '\"'\"'quotes'\"'\"'';"));

        let home_command = TartProvisioner::virtiofs_mount_command("config", "$HOME/.config");
        assert!(home_command.contains("target=\"$HOME\"/'.config';"));
        assert!(!home_command.contains("target='$HOME"));
        assert!(home_command.contains("sudo -n"));
        assert!(home_command.contains("$SUDO /sbin/mount_virtiofs"));
    }

    #[test]
    fn read_only_linux_workspace_uses_guest_dependency_overlay() {
        let provisioner = TartProvisioner::new(
            "demo".to_string(),
            "/workspace".to_string(),
            crate::tart::TartCommand::new(None),
        );
        let config: VmConfig = serde_yaml_ng::from_str(
            "provider: tart\nos: linux\nproject:\n  name: demo\n  workspace_access: read_only\n",
        )
        .unwrap();

        let command = provisioner.workspace_mount_command(&config);

        assert!(command.contains("lowerdir=/mnt/vm-workspace-source"));
        assert!(command.contains("$target/node_modules"));
        assert!(command.contains("remount,bind,ro"));
        #[cfg(unix)]
        assert!(std::process::Command::new("/bin/bash")
            .args(["-n", "-c", &command])
            .status()
            .unwrap()
            .success());
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
        assert!(rendered.contains("VM_SHELL_CONFIG_VERSION=6"));
        assert!(rendered.contains("yocodex()"));
        assert!(rendered.contains("vm_repair_codex_state"));
        assert!(rendered.contains("VM_PROJECT_PATH='/workspace'"));
    }
}
