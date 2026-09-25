use super::TartProvisioner;
use crate::project_plan::ProjectPlan;
use indexmap::IndexMap;
use std::path::PathBuf;
#[cfg(unix)]
use std::process::Command;
use vm_config::config::{
    ImageSpec, ProjectConfig, ServiceConfig, TartConfig, TerminalConfig, VmConfig, VmSettings,
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
    commands.extend(provisioner.package_infrastructure_commands(&config));
    commands.extend(provisioner.project_runtime_commands(&config, &plan));
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
        macos_commands.extend(provisioner.package_infrastructure_commands(&config));
        macos_commands.extend(provisioner.project_runtime_commands(&config, &plan));
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
            image: Some(ImageSpec::String("vibe-tart-sequoia-base".to_string())),
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
            image: Some(ImageSpec::String("vibe-tart-linux-base".to_string())),
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
            image: Some(ImageSpec::String("custom-base".to_string())),
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
            image: Some(ImageSpec::String("custom-team-base".to_string())),
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
            image: Some(ImageSpec::String("vibe-tart-sequoia-base".to_string())),
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
