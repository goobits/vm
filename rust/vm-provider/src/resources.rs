// Embedded resources for VM provisioning
// These are compiled into the binary for portability

#[cfg(feature = "docker")]
use std::fs;
#[cfg(feature = "docker")]
use std::path::Path;
#[cfg(feature = "docker")]
use vm_core::error::Result;

#[cfg(any(feature = "docker", test))]
pub const ANSIBLE_PLAYBOOK: &str = include_str!("resources/ansible/playbook.yml");
#[cfg(any(feature = "docker", test))]
pub const MANAGE_SERVICE_TASK: &str = include_str!("resources/ansible/tasks/manage-service.yml");
#[cfg(any(feature = "docker", test))]
pub const SERVICE_DEFINITIONS: &str = include_str!("resources/services/service_definitions.yml");
pub const ZSHRC_TEMPLATE: &str = include_str!("resources/templates/zshrc.j2");
#[cfg(any(feature = "tart", test))]
pub(crate) const SHELL_CONFIG_VERSION: &str = "6";
pub const THEMES_JSON: &str = include_str!("resources/templates/themes.json");
#[cfg(feature = "docker")]
pub const CLAUDE_SETTINGS_TEMPLATE: &str =
    include_str!("resources/settings/claude-settings.json.j2");
pub(crate) const NODE_BOOTSTRAP: &str = include_str!("resources/scripts/bootstrap-node.sh");
pub(crate) const NODE_TOOLCHAIN_INSTALLER: &str =
    include_str!("resources/scripts/install-node-toolchain.sh");
pub(crate) const HOME_STATE_REPAIR: &str = include_str!("resources/scripts/repair-home-state.sh");

/// Copy all embedded resources to the specified directory
#[cfg(feature = "docker")]
pub fn copy_embedded_resources(shared_dir: &Path) -> Result<()> {
    use rayon::prelude::*;

    // Create directory structure in parallel
    let directories = [
        shared_dir.join("ansible"),
        shared_dir.join("ansible").join("tasks"),
        shared_dir.join("services"),
        shared_dir.join("templates"),
        shared_dir.join("settings"),
        shared_dir.join("claude-settings"),
        shared_dir.join("scripts"),
    ];

    directories[..]
        .par_iter()
        .try_for_each(fs::create_dir_all)?;

    // Write embedded resources in parallel
    let file_operations = [
        (directories[0].join("playbook.yml"), ANSIBLE_PLAYBOOK),
        (
            directories[1].join("manage-service.yml"),
            MANAGE_SERVICE_TASK,
        ),
        (
            directories[2].join("service_definitions.yml"),
            SERVICE_DEFINITIONS,
        ),
        (directories[3].join("zshrc.j2"), ZSHRC_TEMPLATE),
        (shared_dir.join("themes.json"), THEMES_JSON),
        (
            directories[5].join("settings.json.j2"),
            CLAUDE_SETTINGS_TEMPLATE,
        ),
        (directories[6].join("bootstrap-node.sh"), NODE_BOOTSTRAP),
        (
            directories[6].join("install-node-toolchain.sh"),
            NODE_TOOLCHAIN_INSTALLER,
        ),
        (
            directories[6].join("repair-home-state.sh"),
            HOME_STATE_REPAIR,
        ),
    ];

    file_operations[..]
        .par_iter()
        .try_for_each(|(path, content)| write_if_changed(path, content))?;

    Ok(())
}

#[cfg(feature = "docker")]
fn write_if_changed(path: &Path, content: &str) -> Result<()> {
    match fs::read(path) {
        Ok(existing) if existing == content.as_bytes() => Ok(()),
        _ => fs::write(path, content).map_err(Into::into),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ANSIBLE_PLAYBOOK, HOME_STATE_REPAIR, NODE_BOOTSTRAP, NODE_TOOLCHAIN_INSTALLER,
        SHELL_CONFIG_VERSION, ZSHRC_TEMPLATE,
    };
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    #[cfg(unix)]
    use std::{fs, path::Path, process::Command};

    #[cfg(unix)]
    fn write_test_command(path: &Path, body: &str) {
        fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn provisioning_does_not_install_managed_tools_directly() {
        assert!(!ANSIBLE_PLAYBOOK.contains("install-ai-tools"));
        assert!(!ANSIBLE_PLAYBOOK.contains("@google/gemini-cli"));
    }

    #[test]
    fn ansible_normalizes_ai_sync_policy_once() {
        assert_eq!(
            ANSIBLE_PLAYBOOK
                .matches("name: Normalize AI sync configuration")
                .count(),
            1
        );
        assert!(ANSIBLE_PLAYBOOK.contains("ai_sync_all_enabled"));
        assert!(ANSIBLE_PLAYBOOK.contains("ai_sync_tools.get('antigravity'"));
        assert!(!ANSIBLE_PLAYBOOK.contains("project_config.host_sync.ai_tools is defined"));
    }

    #[test]
    fn managed_tool_wrappers_are_always_available() {
        let path = ZSHRC_TEMPLATE
            .find("export PATH=\"$HOME/.local/bin:$PATH\"")
            .unwrap();

        assert!(path < ZSHRC_TEMPLATE.find("yoclaude()").unwrap());
        assert!(path < ZSHRC_TEMPLATE.find("yocodex()").unwrap());
        assert_eq!(ZSHRC_TEMPLATE.matches("yoclaude()").count(), 1);
        assert_eq!(ZSHRC_TEMPLATE.matches("yocodex()").count(), 1);
        assert!(ZSHRC_TEMPLATE.contains("Update the Vibe base and recreate"));
        assert_eq!(
            ZSHRC_TEMPLATE
                .matches("export PATH=\"$HOME/.local/bin:$PATH\"")
                .count(),
            1
        );
        assert!(ZSHRC_TEMPLATE.contains(&format!("VM_SHELL_CONFIG_VERSION={SHELL_CONFIG_VERSION}")));
    }

    #[test]
    fn home_repair_is_versioned_and_mount_aware() {
        assert!(HOME_STATE_REPAIR.contains("REPAIR_VERSION=2"));
        assert!(HOME_STATE_REPAIR.contains("home-repair"));
        assert!(HOME_STATE_REPAIR.contains("VM_HOME_REPAIR_FORCE"));
        assert!(HOME_STATE_REPAIR.contains("full_repair"));
        assert!(HOME_STATE_REPAIR.contains("state_fingerprint"));
        assert!(HOME_STATE_REPAIR.contains("find \"$path\" -xdev"));
        assert!(HOME_STATE_REPAIR.contains("quarantine_file"));
        assert!(!HOME_STATE_REPAIR.contains("-name '*.json' -size 0 -delete"));
        assert!(!HOME_STATE_REPAIR.contains("marker.tmp.$$"));
        assert_eq!(ANSIBLE_PLAYBOOK.matches("repair-home-state.sh").count(), 0);
    }

    #[test]
    fn ansible_does_not_interpolate_configuration_into_shell_commands() {
        assert!(!ANSIBLE_PLAYBOOK.contains("pipx install {{"));
        assert!(!ANSIBLE_PLAYBOOK.contains("git config --global user.name {{"));
        assert!(!ANSIBLE_PLAYBOOK.contains("https://sh.rustup.rs"));
        assert!(ANSIBLE_PLAYBOOK.contains("checksum: 'sha256:https://static.rust-lang.org"));
        assert!(!super::MANAGE_SERVICE_TASK.contains("shell: \"{{ item }}\""));
        assert!(!super::SERVICE_DEFINITIONS.contains("curl -fsSL"));
    }

    #[test]
    fn ansible_consumes_the_shared_host_project_plan() {
        assert!(ANSIBLE_PLAYBOOK.contains("_vm_project_plan"));
        assert!(ANSIBLE_PLAYBOOK.contains("_vm_cache_environment"));
        assert!(ANSIBLE_PLAYBOOK.contains("cache_environment | combine"));
        assert!(ANSIBLE_PLAYBOOK.contains("Ensure managed guest cache directories are writable"));
        assert!(!ANSIBLE_PLAYBOOK.contains("project_package_files"));
        assert!(!ANSIBLE_PLAYBOOK.contains("Inspect project package files"));
    }

    #[test]
    fn node_bootstrap_is_shared_and_portable() {
        assert!(ANSIBLE_PLAYBOOK.contains("scripts/install-node-toolchain.sh"));
        assert!(ANSIBLE_PLAYBOOK.contains("scripts/bootstrap-node.sh"));
        assert!(!ANSIBLE_PLAYBOOK.contains("tasks/node-toolchain.yml"));
        assert!(!ANSIBLE_PLAYBOOK.contains("tasks/bootstrap-node.yml"));
        assert!(NODE_TOOLCHAIN_INSTALLER.contains("VM_NODE_TOOLCHAIN_CURRENT=1"));
        assert!(NODE_TOOLCHAIN_INSTALLER.contains("PROFILE=\"$installer_profile\""));
        assert!(NODE_TOOLCHAIN_INSTALLER.contains("installer_status=$?"));
        assert!(NODE_BOOTSTRAP.contains("VM_BOOTSTRAP_DEPENDENCIES_CURRENT=1"));
        assert!(NODE_BOOTSTRAP.contains("shasum -a 256"));
        assert!(!NODE_BOOTSTRAP.contains("mapfile"));
    }

    #[cfg(unix)]
    #[test]
    fn node_toolchain_skips_current_versions() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let bin = root.path().join("bin");
        fs::create_dir_all(home.join(".nvm")).unwrap();
        fs::create_dir(&bin).unwrap();
        fs::write(
            home.join(".nvm/nvm.sh"),
            "nvm() { case \"$1\" in version) echo v22.0.0 ;; use|alias) return 0 ;; *) return 1 ;; esac; }\n",
        )
        .unwrap();
        write_test_command(&bin.join("npm"), "[ \"$1\" = --version ] && echo 10.0.0");
        write_test_command(&bin.join("pnpm"), "[ \"$1\" = --version ] && echo 10.12.3");
        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );

        let output = Command::new("/bin/bash")
            .args(["-c", NODE_TOOLCHAIN_INSTALLER])
            .env("HOME", &home)
            .env("NVM_DIR", home.join(".nvm"))
            .env("PATH", path)
            .env("VM_NODE_VERSION", "22")
            .env("VM_NPM_VERSION", "10.0.0")
            .env("VM_PNPM_VERSION", "10.12.3")
            .output()
            .unwrap();

        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("VM_NODE_TOOLCHAIN_CURRENT=1"));
    }

    #[cfg(unix)]
    #[test]
    fn node_toolchain_accepts_a_valid_runtime_after_installer_status_three() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let bin = root.path().join("bin");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&bin).unwrap();
        write_test_command(
            &bin.join("curl"),
            r#"destination=
while [ "$#" -gt 0 ]; do
  if [ "$1" = -o ]; then shift; destination=$1; fi
  shift
done
cat > "$destination" <<'INSTALLER'
mkdir -p "$NVM_DIR"
cat > "$NVM_DIR/nvm.sh" <<'NVM'
nvm() {
  case "$1" in
    version)
      if [ -f "$NVM_DIR/installed" ]; then echo v22.0.0; else echo N/A; return 3; fi
      ;;
    install) touch "$NVM_DIR/installed" ;;
    use|alias) return 0 ;;
    *) return 1 ;;
  esac
}
NVM
exit 3
INSTALLER"#,
        );
        write_test_command(&bin.join("npm"), "[ \"$1\" = --version ] && echo 10.0.0");
        write_test_command(&bin.join("pnpm"), "[ \"$1\" = --version ] && echo 10.12.3");
        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );

        let output = Command::new("/bin/bash")
            .args(["-c", NODE_TOOLCHAIN_INSTALLER])
            .env("HOME", &home)
            .env("NVM_DIR", home.join(".nvm"))
            .env("PATH", path)
            .env("VM_NODE_VERSION", "22")
            .env("VM_NPM_VERSION", "10.0.0")
            .env("VM_PNPM_VERSION", "10.12.3")
            .output()
            .unwrap();

        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("VM_NODE_TOOLCHAIN_CHANGED=1"));
    }

    #[cfg(unix)]
    #[test]
    fn node_dependency_bootstrap_reuses_its_fingerprint() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let project = root.path().join("project");
        let bin = root.path().join("bin");
        fs::create_dir_all(home.join(".nvm")).unwrap();
        fs::create_dir(&project).unwrap();
        fs::create_dir(&bin).unwrap();
        fs::write(home.join(".nvm/nvm.sh"), "").unwrap();
        fs::write(project.join("package.json"), "{}\n").unwrap();
        fs::write(project.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n").unwrap();
        write_test_command(&bin.join("node"), "echo v22.0.0");
        write_test_command(
            &bin.join("pnpm"),
            "if [ \"$1\" = --version ]; then echo 10.12.3; else mkdir -p node_modules && touch node_modules/installed; fi",
        );
        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let run = || {
            Command::new("/bin/bash")
                .args(["-c", NODE_BOOTSTRAP])
                .env("HOME", &home)
                .env("NVM_DIR", home.join(".nvm"))
                .env("PATH", &path)
                .env("VM_PROJECT_PATH", &project)
                .env("VM_NODE_DEPENDENCY_MANAGER", "pnpm")
                .output()
                .unwrap()
        };

        let first = run();
        assert!(first.status.success());
        assert!(
            String::from_utf8_lossy(&first.stdout).contains("VM_BOOTSTRAP_DEPENDENCIES_CHANGED=1")
        );
        let second = run();
        assert!(second.status.success());
        assert!(
            String::from_utf8_lossy(&second.stdout).contains("VM_BOOTSTRAP_DEPENDENCIES_CURRENT=1")
        );
    }
}
