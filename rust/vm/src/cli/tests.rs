use super::{
    Args, BaseSubcommand, Command, ConfigSubcommand, DbSubcommand, EnvironmentKind,
    PackageInfrastructureRuntime, PackagesSubcommand, PluginSubcommand, SystemSubcommand,
    ToolsSubcommand,
};
use clap::Parser;

#[test]
fn run_parses_kind_and_humane_name() {
    assert!(matches!(
        Args::parse_from(["vm", "run", "linux", "as", "backend"]).command,
        Command::Run {
            kind: EnvironmentKind::Linux,
            words,
            ..
        } if words == ["as", "backend"]
    ));
}

#[test]
fn shell_and_ssh_parse_the_same_environment() {
    for name in ["shell", "ssh"] {
        assert!(matches!(
            Args::parse_from(["vm", name, "backend"]).command,
            Command::Shell {
                environment: Some(environment),
                ..
            } if environment == "backend"
        ));
    }
}

#[test]
fn start_and_ssh_accept_the_project_default() {
    assert!(matches!(
        Args::parse_from(["vm", "start"]).command,
        Command::Start {
            environment: None,
            no_wait: false,
            ..
        }
    ));
    assert!(matches!(
        Args::parse_from(["vm", "ssh"]).command,
        Command::Shell {
            environment: None,
            ..
        }
    ));
}

#[test]
fn ssh_alias_parses_command_execution() {
    assert!(matches!(
        Args::parse_from(["vm", "ssh", "-e", "echo hello"]).command,
        Command::Shell {
            command: Some(command),
            ..
        } if command == "echo hello"
    ));
}

#[test]
fn list_aliases_parse_filters() {
    assert!(matches!(
        Args::parse_from(["vm", "ls", "--all"]).command,
        Command::List {
            all: true,
            raw: false
        }
    ));
    assert!(matches!(
        Args::parse_from(["vm", "list", "--raw"]).command,
        Command::List {
            all: false,
            raw: true
        }
    ));
}

#[test]
fn destroy_alias_parses_as_remove() {
    assert!(matches!(
        Args::parse_from(["vm", "destroy", "backend", "--force"]).command,
        Command::Remove {
            environment: Some(environment),
            force: true
        } if environment == "backend"
    ));
}

#[test]
fn lifecycle_commands_parse() {
    assert!(matches!(
        Args::parse_from(["vm", "create", "--force"]).command,
        Command::Create { force: true, .. }
    ));
    assert!(matches!(
        Args::parse_from(["vm", "start", "backend", "--no-wait"]).command,
        Command::Start {
            environment: Some(environment),
            no_wait: true,
            ..
        } if environment == "backend"
    ));
    assert!(matches!(
        Args::parse_from(["vm", "status", "backend"]).command,
        Command::Status {
            environment: Some(environment)
        } if environment == "backend"
    ));
}

#[test]
fn stop_legacy_aliases_parse() {
    for alias in ["down", "halt"] {
        assert!(matches!(
            Args::parse_from(["vm", alias, "backend"]).command,
            Command::Stop {
                environment: Some(environment),
                ..
            } if environment == "backend"
        ));
    }
}

#[test]
fn exec_parses_command() {
    assert!(matches!(
        Args::parse_from(["vm", "exec", "backend", "--", "npm", "test"]).command,
        Command::Exec {
            environment: Some(environment),
            command,
            ..
        } if environment == "backend" && command == ["npm", "test"]
    ));
}

#[test]
fn exec_uses_default_environment_when_omitted() {
    assert!(matches!(
        Args::parse_from(["vm", "exec", "--", "npm", "test"]).command,
        Command::Exec {
            environment: None,
            command,
            ..
        } if command == ["npm", "test"]
    ));
}

#[test]
fn fleet_is_a_shared_targeting_flag() {
    assert!(matches!(
        Args::parse_from([
            "vm",
            "exec",
            "--fleet",
            "--provider",
            "docker",
            "--pattern",
            "app-*",
            "--",
            "npm",
            "test",
        ])
        .command,
        Command::Exec { fleet, command, .. }
            if fleet.fleet
                && fleet.provider.as_deref() == Some("docker")
                && fleet.pattern.as_deref() == Some("app-*")
                && command == ["npm", "test"]
    ));
    assert!(Args::try_parse_from(["vm", "stop", "backend", "--fleet"]).is_err());
    assert!(Args::try_parse_from(["vm", "fleet", "stop"]).is_err());
}

#[test]
fn save_parses_humane_snapshot_name() {
    assert!(matches!(
        Args::parse_from(["vm", "save", "backend", "as", "stable"]).command,
        Command::Save { words, .. } if words == ["backend", "as", "stable"]
    ));
}

#[test]
fn system_base_build_parses_macos_guest_os() {
    assert!(matches!(
        Args::parse_from([
            "vm",
            "system",
            "base",
            "build",
            "vibe",
            "--provider",
            "tart",
            "--guest-os",
            "macos",
        ])
        .command,
        Command::System {
            command: SystemSubcommand::Base {
                command: BaseSubcommand::Build {
                    preset,
                    provider,
                    guest_os
                }
            }
        } if preset == "vibe" && provider == "tart" && guest_os == "macos"
    ));
}

#[test]
fn packages_up_parses_tart_appliance() {
    assert!(matches!(
        Args::parse_from([
            "vm",
            "packages",
            "up",
            "--runtime",
            "tart",
            "--port",
            "4080",
        ])
        .command,
        Command::Packages {
            command: PackagesSubcommand::Up {
                runtime: PackageInfrastructureRuntime::Tart,
                port: 4080,
                ..
            }
        }
    ));
}

#[test]
fn package_source_roots_parse_as_global_string_array() {
    assert!(matches!(
        Args::parse_from([
            "vm",
            "config",
            "set",
            "packages.source_roots",
            "/srv/packages",
            "/opt/shared",
            "--global",
        ])
        .command,
        Command::Config {
            command: ConfigSubcommand::Set {
                field,
                values,
                global: true,
            }
        } if field == "packages.source_roots"
            && values == ["/srv/packages", "/opt/shared"]
    ));
}

#[test]
fn package_registration_parses_explicit_and_discovery_modes() {
    assert!(matches!(
        Args::parse_from([
            "vm",
            "packages",
            "register",
            "auth",
            "--ecosystem",
            "cargo",
            "--repository",
            "https://example.com/auth.git",
        ])
        .command,
        Command::Packages {
            command: PackagesSubcommand::Register {
                targets,
                ecosystem: Some(ecosystem),
                repository: Some(repository),
                recursive: false,
                ..
            }
        } if targets == ["auth"]
            && ecosystem == "cargo"
            && repository == "https://example.com/auth.git"
    ));
    assert!(matches!(
        Args::parse_from([
            "vm",
            "packages",
            "register",
            "./packages/auth",
            "./packages/ui",
            "--recursive",
        ])
        .command,
        Command::Packages {
            command: PackagesSubcommand::Register {
                targets,
                repository: None,
                recursive: true,
                ..
            }
        } if targets == ["./packages/auth", "./packages/ui"]
    ));
}

#[test]
fn package_auth_can_import_the_active_github_credential() {
    assert!(matches!(
        Args::parse_from(["vm", "packages", "auth", "--github"]).command,
        Command::Packages {
            command: PackagesSubcommand::Auth {
                github: true,
                token_file: None,
                clear: false,
                ..
            }
        }
    ));
}

#[test]
fn package_init_parses_the_source_shelf() {
    assert!(matches!(
        Args::parse_from(["vm", "packages", "init", "/srv/packages"]).command,
        Command::Packages {
            command: PackagesSubcommand::Init { source_root, port, .. }
        } if source_root == std::path::Path::new("/srv/packages") && port == 3080
    ));
    assert!(matches!(
        Args::parse_from([
            "vm",
            "packages",
            "init",
            "/srv/packages",
            "--runtime",
            "docker",
            "--port",
            "39081",
            "--registry-image",
            "registry:test",
            "--job-image",
            "jobs:test",
        ])
        .command,
        Command::Packages {
            command: PackagesSubcommand::Init {
                source_root,
                runtime: PackageInfrastructureRuntime::Docker,
                port: 39081,
                registry_image: Some(registry_image),
                job_image: Some(job_image),
            }
        } if source_root == std::path::Path::new("/srv/packages")
            && registry_image == "registry:test"
            && job_image == "jobs:test"
    ));
}

#[test]
fn package_release_accepts_an_inferred_checkout() {
    assert!(matches!(
        Args::parse_from(["vm", "packages", "release"]).command,
        Command::Packages {
            command: PackagesSubcommand::Release
        }
    ));
}

#[test]
fn package_doctor_parses_safe_fix_mode() {
    assert!(matches!(
        Args::parse_from(["vm", "packages", "doctor", "--fix"]).command,
        Command::Packages {
            command: PackagesSubcommand::Doctor { fix: true, .. }
        }
    ));
}

#[test]
fn package_checkout_parses_isolated_work_request() {
    assert!(matches!(
        Args::parse_from(["vm", "packages", "checkout", "auth"]).command,
        Command::Packages {
            command: PackagesSubcommand::Checkout { source }
        } if source == "auth"
    ));
    assert!(
        Args::try_parse_from(["vm", "packages", "checkout", "auth", "--agent", "agent-17"])
            .is_err()
    );
}

#[test]
fn package_cancel_parses_directory_inferred_workflow() {
    assert!(matches!(
        Args::parse_from(["vm", "packages", "cancel"]),
        Args {
            command: Command::Packages {
                command: PackagesSubcommand::Cancel
            },
            ..
        }
    ));
    assert!(Args::try_parse_from(["vm", "packages", "release", "checkout-auth-1"]).is_err());
}

#[test]
fn package_recovery_commands_parse() {
    assert!(matches!(
        Args::parse_from(["vm", "packages", "restore", "backup-20260810"]),
        Args {
            command: Command::Packages {
                command: PackagesSubcommand::Restore { backup_id, .. }
            },
            ..
        } if backup_id == "backup-20260810"
    ));
}

#[test]
fn package_inventory_commands_parse() {
    assert!(matches!(
        Args::parse_from([
            "vm",
            "packages",
            "consumer",
            "register",
            "project-a",
            "--repository",
            "https://example.com/project-a.git",
            "--dependency",
            "auth@1.4.2",
        ]),
        Args {
            command: Command::Packages {
                command: PackagesSubcommand::Consumer { .. }
            },
            ..
        }
    ));
}

#[test]
fn tool_refresh_status_and_batch_update_commands_parse() {
    assert!(matches!(
        Args::parse_from([
            "vm",
            "tools",
            "register",
            "agent-skills",
            "--kind",
            "collection",
            "--repository",
            "https://example.com/agent-skills.git",
        ])
        .command,
        Command::Tools {
            command: ToolsSubcommand::Register { name, kind, .. }
        } if name == "agent-skills" && kind == "collection"
    ));
    assert!(matches!(
        Args::parse_from(["vm", "tools", "list"]).command,
        Command::Tools {
            command: ToolsSubcommand::List
        }
    ));
    assert!(matches!(
        Args::parse_from(["vm", "tools", "show", "codex"]).command,
        Command::Tools {
            command: ToolsSubcommand::Show { name }
        } if name == "codex"
    ));
    assert!(matches!(
        Args::parse_from(["vm", "tools", "refresh"]).command,
        Command::Tools {
            command: ToolsSubcommand::Refresh { quiet: false }
        }
    ));
    assert!(matches!(
        Args::parse_from(["vm", "tools", "status", "backend"]).command,
        Command::Tools {
            command: ToolsSubcommand::Status {
                environment: Some(environment)
            }
        } if environment == "backend"
    ));
    assert!(matches!(
        Args::parse_from(["vm", "tools", "update", "agent-skills", "--background"]).command,
        Command::Tools {
            command: ToolsSubcommand::Update {
                tools,
                to,
                include_stopped: false,
                background: true,
                ..
            }
        } if tools == ["agent-skills"] && to.is_empty()
    ));
    assert!(matches!(
        Args::parse_from(["vm", "tools", "update"]).command,
        Command::Tools {
            command: ToolsSubcommand::Update {
                tools,
                to,
                include_stopped: false,
                ..
            }
        } if tools.is_empty() && to.is_empty()
    ));
    assert!(matches!(
        Args::parse_from([
            "vm",
            "tools",
            "update",
            "agent-skills",
            "helper",
            "--to",
            "backend",
            "--to",
            "worker",
        ])
        .command,
        Command::Tools {
            command: ToolsSubcommand::Update {
                tools,
                to,
                include_stopped: false,
                background: false,
                ..
            }
        } if tools == ["agent-skills", "helper"] && to == ["backend", "worker"]
    ));
    assert!(matches!(
        Args::parse_from(["vm", "tools", "update", "--to", "backend", "agent-skills"])
            .command,
        Command::Tools {
            command: ToolsSubcommand::Update { tools, to, .. }
        } if tools == ["agent-skills"] && to == ["backend"]
    ));
    assert!(matches!(
        Args::parse_from([
            "vm",
            "tools",
            "update",
            "agent-skills",
            "--include-stopped",
        ])
        .command,
        Command::Tools {
            command: ToolsSubcommand::Update {
                tools,
                include_stopped: true,
                ..
            }
        } if tools == ["agent-skills"]
    ));
    assert!(matches!(
        Args::parse_from([
            "vm",
            "tools",
            "update",
            "--fleet",
            "--provider",
            "docker",
        ])
        .command,
        Command::Tools {
            command: ToolsSubcommand::Update {
                tools,
                to,
                include_stopped: false,
                fleet,
                background: false,
            }
        } if tools.is_empty() && to.is_empty() && fleet.fleet && fleet.provider.as_deref() == Some("docker")
    ));
    assert!(Args::try_parse_from(["vm", "tools", "update", "--all"]).is_err());
    assert!(Args::try_parse_from([
        "vm",
        "tools",
        "update",
        "agent-skills",
        "--to",
        "backend",
        "--fleet",
    ])
    .is_err());
}

#[test]
fn plugin_install_parses() {
    assert!(matches!(
        Args::parse_from(["vm", "plugin", "install", "/path/to/plugin"]).command,
        Command::Plugin {
            command: PluginSubcommand::Install { source_path }
        } if source_path == "/path/to/plugin"
    ));
}

#[test]
fn db_remains_top_level_plugin_command() {
    assert!(matches!(
        Args::parse_from(["vm", "db", "ls"]).command,
        Command::Db {
            command: DbSubcommand::Ls
        }
    ));
}

#[test]
fn config_render_parses_instance() {
    assert!(matches!(
        Args::parse_from(["vm", "config", "render", "--instance", "feature"]).command,
        Command::Config {
            command: ConfigSubcommand::Render {
                instance: Some(instance)
            }
        } if instance == "feature"
    ));
}

#[test]
fn doctor_parses_pnpm_store_maintenance_target() {
    assert!(matches!(
        Args::parse_from([
            "vm",
            "doctor",
            "--prune-pnpm-store",
            "--container",
            "feature",
        ])
        .command,
        Command::Doctor {
            prune_pnpm_store: true,
            container: Some(container),
            ..
        } if container == "feature"
    ));
}

#[test]
fn shell_rejects_removed_refresh_flags() {
    assert!(Args::try_parse_from(["vm", "ssh", "--force-refresh"]).is_err());
    assert!(Args::try_parse_from(["vm", "ssh", "--no-refresh"]).is_err());
}
