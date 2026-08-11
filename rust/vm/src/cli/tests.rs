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
            no_wait: false
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
            no_wait: true
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
                environment: Some(environment)
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
            command
        } if environment == "backend" && command == ["npm", "test"]
    ));
}

#[test]
fn exec_uses_default_environment_when_omitted() {
    assert!(matches!(
        Args::parse_from(["vm", "exec", "--", "npm", "test"]).command,
        Command::Exec {
            environment: None,
            command
        } if command == ["npm", "test"]
    ));
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
fn package_checkout_parses_isolated_work_request() {
    assert!(matches!(
        Args::parse_from([
            "vm",
            "packages",
            "checkout",
            "auth",
            "--agent",
            "agent-17",
            "--consumer",
            "project-a",
            "--task",
            "fix refresh",
        ])
        .command,
        Command::Packages {
            command: PackagesSubcommand::Checkout {
                package,
                agent,
                consumer: Some(consumer),
                task,
            }
        } if package == "auth"
            && agent == "agent-17"
            && consumer == "project-a"
            && task == "fix refresh"
    ));
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
    assert!(matches!(
        Args::parse_from(["vm", "packages", "cleanup", "pkg-auth-1"]),
        Args {
            command: Command::Packages {
                command: PackagesSubcommand::Cleanup { checkout_id }
            },
            ..
        } if checkout_id == "pkg-auth-1"
    ));
}

#[test]
fn package_inventory_and_rollout_commands_parse() {
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
    assert!(matches!(
        Args::parse_from(["vm", "packages", "rollout", "auth@1.5.0", "--to", "project-a"]),
        Args {
            command: Command::Packages {
                command: PackagesSubcommand::Rollout { target, consumer }
            },
            ..
        } if target == "auth@1.5.0" && consumer == "project-a"
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
        Args::parse_from(["vm", "tools", "publish", "agent-skills"]).command,
        Command::Tools {
            command: ToolsSubcommand::Publish { name }
        } if name == "agent-skills"
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
        Args::parse_from(["vm", "tools", "update", "backend", "--all", "--background"])
            .command,
        Command::Tools {
            command: ToolsSubcommand::Update {
                environment: Some(environment),
                all: true,
                background: true
            }
        } if environment == "backend"
    ));
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
