use super::{
    Args, BaseSubcommand, Command, ConfigSubcommand, DbSubcommand, EnvironmentKind,
    PackageInfrastructureEngine, PackagesSubcommand, PluginSubcommand, SystemSubcommand,
    ToolsSubcommand,
};
use clap::Parser;

#[test]
fn failed_consumer_updates_can_be_retried_by_project_name() {
    assert!(matches!(
        Args::parse_from(["vm", "packages", "consumers", "retry", "project-a"]).command,
        Command::Packages { command: PackagesSubcommand::Consumers {
            command: super::PackageConsumerSubcommand::Retry { name }
        }} if name == "project-a"
    ));
}

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
fn shell_accepts_an_explicit_or_default_environment() {
    assert!(matches!(
        Args::parse_from(["vm", "shell", "backend"]).command,
        Command::Shell { environment: Some(environment), .. } if environment == "backend"
    ));
    assert!(matches!(
        Args::parse_from(["vm", "shell"]).command,
        Command::Shell {
            environment: None,
            ..
        }
    ));
    assert!(Args::try_parse_from(["vm", "ssh"]).is_err());
    assert!(Args::try_parse_from(["vm", "ls"]).is_err());
}

#[test]
fn remove_parses_environment_and_force() {
    assert!(matches!(
        Args::parse_from(["vm", "remove", "backend", "--force"]).command,
        Command::Remove {
            environment: Some(environment),
            force: true
        } if environment == "backend"
    ));
}

#[test]
fn lifecycle_commands_parse() {
    assert!(matches!(
        Args::parse_from(["vm", "start", "backend", "--no-wait"]).command,
        Command::Start {
            environments,
            no_wait: true,
            ..
        } if environments == ["backend"]
    ));
    assert!(matches!(
        Args::parse_from(["vm", "status", "backend"]).command,
        Command::Status {
            environments,
            ..
        } if environments == ["backend"]
    ));
}

#[test]
fn project_selection_and_init_parse() {
    let args = Args::parse_from(["vm", "--project", "demo", "status", "dev", "test"]);
    assert_eq!(args.project.as_deref(), Some(std::path::Path::new("demo")));
    assert!(matches!(
        args.command,
        Command::Status { environments, .. } if environments == ["dev", "test"]
    ));
    assert!(matches!(
        Args::parse_from(["vm", "init", "./example"]).command,
        Command::Init { path: Some(path) } if path == std::path::Path::new("./example")
    ));
    assert!(
        Args::try_parse_from(["vm", "--project", "demo", "--config", "vm.yaml", "list"]).is_err()
    );
    assert!(matches!(
        Args::parse_from(["vm", "list", "--all-projects"]).command,
        Command::List {
            all_projects: true,
            ..
        }
    ));
    assert!(Args::try_parse_from(["vm", "list", "--all"]).is_err());
}

#[test]
fn stop_parses_named_environments() {
    assert!(matches!(
        Args::parse_from(["vm", "stop", "backend", "test"]).command,
        Command::Stop {
            environments,
            ..
        } if environments == ["backend", "test"]
    ));
}

#[test]
fn retired_lifecycle_aliases_are_rejected() {
    for command in ["get-sync-directory", "down", "halt", "rm", "destroy"] {
        assert!(Args::try_parse_from(["vm", command, "backend"]).is_err());
    }
}

#[test]
fn create_requires_exactly_one_source() {
    assert!(matches!(
        Args::parse_from(["vm", "create", "dev", "--provider", "docker", "--image", "debian:bookworm"]).command,
        Command::Create { name, provider, image: Some(image), snapshot: None, .. }
            if name == "dev" && provider == "docker" && image == "debian:bookworm"
    ));
    assert!(Args::try_parse_from(["vm", "create", "dev", "--provider", "docker"]).is_err());
    assert!(Args::try_parse_from([
        "vm",
        "create",
        "dev",
        "--provider",
        "docker",
        "--image",
        "one",
        "--snapshot",
        "two"
    ])
    .is_err());
}

#[test]
fn exec_accepts_repeated_explicit_environments_or_a_default() {
    assert!(matches!(
        Args::parse_from(["vm", "exec", "--env", "backend", "--env", "test", "--", "npm", "test"]).command,
        Command::Exec { environments, command, .. }
            if environments == ["backend", "test"] && command == ["npm", "test"]
    ));
    assert!(matches!(
        Args::parse_from(["vm", "exec", "--", "npm", "test"]).command,
        Command::Exec { environments, command, .. } if environments.is_empty() && command == ["npm", "test"]
    ));
    assert!(Args::try_parse_from(["vm", "exec", "backend", "--", "npm"]).is_err());
}

#[test]
fn fleet_is_a_shared_targeting_flag() {
    assert!(matches!(
        Args::parse_from([
            "vm",
            "exec",
            "--all-envs",
            "--match-provider",
            "docker",
            "--match",
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
    assert!(Args::try_parse_from(["vm", "stop", "backend", "--all-envs"]).is_err());
    assert!(Args::try_parse_from(["vm", "fleet", "stop"]).is_err());
}

#[test]
fn snapshot_commands_are_grouped() {
    assert!(matches!(
        Args::parse_from(["vm", "snapshots", "create", "stable", "--env", "backend"]).command,
        Command::Snapshots { command: super::SnapshotSubcommand::Create { name, env: Some(env), .. } }
            if name == "stable" && env == "backend"
    ));
    assert!(matches!(
        Args::parse_from(["vm", "snapshots", "import", "stable.tar.gz", "--name", "stable"]).command,
        Command::Snapshots { command: super::SnapshotSubcommand::Import { archive, name } }
            if archive == std::path::Path::new("stable.tar.gz") && name == "stable"
    ));
    assert!(Args::try_parse_from(["vm", "save", "as", "stable"]).is_err());
}

#[test]
fn system_images_build_parses_macos_guest_os() {
    assert!(matches!(
        Args::parse_from([
            "vm",
            "system",
            "images",
            "build",
            "vibe",
            "--provider",
            "tart",
            "--guest-os",
            "macos",
        ])
        .command,
        Command::System {
            command: SystemSubcommand::Images {
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
fn system_images_build_accepts_podman() {
    assert!(matches!(
        Args::parse_from(["vm", "system", "images", "build", "vibe", "--provider", "podman"])
            .command,
        Command::System {
            command: SystemSubcommand::Images {
                command: BaseSubcommand::Build { provider, .. }
            }
        } if provider == "podman"
    ));
}

#[test]
fn system_images_validate_is_not_a_public_command() {
    assert!(Args::try_parse_from(["vm", "system", "images", "validate", "vibe"]).is_err());
}

#[test]
fn system_storage_requires_an_exact_resource_id_for_removal() {
    assert!(Args::try_parse_from(["vm", "system", "storage", "list"]).is_ok());
    assert!(Args::try_parse_from(["vm", "system", "storage", "remove"]).is_err());
    assert!(Args::try_parse_from([
        "vm",
        "system",
        "storage",
        "remove",
        "docker:volume:demo_data",
        "--yes"
    ])
    .is_ok());
}

#[test]
fn packages_up_parses_podman_engine() {
    assert!(matches!(
        Args::parse_from(["vm", "packages", "up", "--engine", "podman", "--port", "4080",]).command,
        Command::Packages {
            command: PackagesSubcommand::Up {
                engine: PackageInfrastructureEngine::Podman,
                port: Some(4080),
                ..
            }
        }
    ));
    assert!(matches!(
        Args::parse_from(["vm", "packages", "up"]).command,
        Command::Packages {
            command: PackagesSubcommand::Up { port: None, .. }
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
            "--scope",
            "user",
        ])
        .command,
        Command::Config {
            command: ConfigSubcommand::Set {
                field,
                values,
                scope: super::ConfigWriteScope::User,
                value_json: None,
            }
        } if field == "packages.source_roots"
            && values == ["/srv/packages", "/opt/shared"]
    ));
    assert!(matches!(
        Args::parse_from([
            "vm",
            "config",
            "set",
            "packages.canonical_sources",
            "/srv/projects/typemill",
            "/srv/projects/codeatlas",
            "--scope",
            "user",
        ])
        .command,
        Command::Config {
            command: ConfigSubcommand::Set {
                field,
                values,
                scope: super::ConfigWriteScope::User,
                value_json: None,
            }
        } if field == "packages.canonical_sources"
            && values == ["/srv/projects/typemill", "/srv/projects/codeatlas"]
    ));
}

#[test]
fn config_uses_canonical_scopes_and_resource_groups() {
    assert!(matches!(
        Args::parse_from(["vm", "config", "show", "--scope", "effective"]).command,
        Command::Config {
            command: ConfigSubcommand::Show {
                scope: super::ConfigReadScope::Effective
            }
        }
    ));
    assert!(matches!(
        Args::parse_from(["vm", "config", "profiles", "set-default", "dev"]).command,
        Command::Config { command: ConfigSubcommand::Profiles { command: super::ConfigProfileSubcommand::SetDefault { name } } } if name == "dev"
    ));
    assert!(matches!(
        Args::parse_from(["vm", "config", "presets", "apply", "nodejs", "python", "--scope", "user"]).command,
        Command::Config { command: ConfigSubcommand::Presets { command: super::ConfigPresetSubcommand::Apply { names, scope: super::ConfigWriteScope::User } } } if names == ["nodejs", "python"]
    ));
    assert!(Args::try_parse_from([
        "vm",
        "config",
        "set",
        "provider",
        "docker",
        "--scope",
        "effective"
    ])
    .is_err());
    assert!(Args::try_parse_from(["vm", "config", "profile", "ls"]).is_err());
    assert!(matches!(
        Args::parse_from(["vm", "config", "set", "networking.networks", "--value-json", "[\"dev\"]"]).command,
        Command::Config { command: ConfigSubcommand::Set { value_json: Some(json), values, .. } }
        if json == "[\"dev\"]" && values.is_empty()
    ));
    assert!(Args::try_parse_from([
        "vm",
        "config",
        "set",
        "vm.memory",
        "4096",
        "--value-json",
        "[]"
    ])
    .is_err());
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
    let help = Args::try_parse_from(["vm", "packages", "register", "--help"])
        .unwrap_err()
        .to_string();
    assert!(help.contains("remember local Git roots as read-only workspaces"));
}

#[test]
fn package_auth_can_import_the_active_github_credential() {
    assert!(matches!(
        Args::parse_from(["vm", "packages", "auth", "login"]).command,
        Command::Packages {
            command: PackagesSubcommand::Auth {
                command: super::PackageAuthSubcommand::Login {
                    token_stdin: false,
                    token_file: None
                }
            }
        }
    ));
    assert!(matches!(
        Args::parse_from(["vm", "packages", "auth", "status"]).command,
        Command::Packages {
            command: PackagesSubcommand::Auth {
                command: super::PackageAuthSubcommand::Status
            }
        }
    ));
    assert!(matches!(
        Args::parse_from(["vm", "packages", "auth", "logout"]).command,
        Command::Packages {
            command: PackagesSubcommand::Auth {
                command: super::PackageAuthSubcommand::Logout
            }
        }
    ));
    assert!(Args::try_parse_from([
        "vm",
        "packages",
        "auth",
        "login",
        "--token-stdin",
        "--token-file",
        "token.txt"
    ])
    .is_err());
}

#[test]
fn package_init_parses_the_source_shelf() {
    assert!(matches!(
        Args::parse_from(["vm", "packages", "service", "init", "--source-root", "/srv/packages"]).command,
        Command::Packages {
            command: PackagesSubcommand::Service { command: super::PackageServiceSubcommand::Init { source_root, port, .. } }
        } if source_root == std::path::Path::new("/srv/packages") && port == 3080
    ));
    assert!(matches!(
        Args::parse_from([
            "vm",
            "packages",
            "service",
            "init",
            "--source-root",
            "/srv/packages",
            "--engine",
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
            command: PackagesSubcommand::Service { command: super::PackageServiceSubcommand::Init {
                source_root,
                engine: PackageInfrastructureEngine::Docker,
                port: 39081,
                registry_image: Some(registry_image),
                job_image: Some(job_image),
            } }
        } if source_root == std::path::Path::new("/srv/packages")
            && registry_image == "registry:test"
            && job_image == "jobs:test"
    ));
}

#[test]
fn retired_package_flags_are_rejected() {
    assert!(Args::try_parse_from(["vm", "packages", "up", "--review-image", "jobs:test"]).is_err());
    assert!(
        Args::try_parse_from(["vm", "packages", "auth", "--git-token-file", "/tmp/token"]).is_err()
    );
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
        Args::parse_from(["vm", "packages", "service", "doctor", "--fix"]).command,
        Command::Packages {
            command: PackagesSubcommand::Service {
                command: super::PackageServiceSubcommand::Doctor { fix: true }
            }
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
fn package_open_parses_direct_workspace_request() {
    assert!(matches!(
        Args::parse_from(["vm", "packages", "open", "auth"]).command,
        Command::Packages {
            command: PackagesSubcommand::Open { source }
        } if source == "auth"
    ));
    assert!(
        Args::try_parse_from(["vm", "packages", "open", "auth", "--environment", "other",])
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
        Args::parse_from(["vm", "packages", "service", "backups", "restore", "backup-20260810"]),
        Args {
            command: Command::Packages {
                command: PackagesSubcommand::Service { command: super::PackageServiceSubcommand::Backups { command: super::PackageBackupSubcommand::Restore { name: backup_id } } }
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
            "consumers",
            "register",
            "project-a",
            "--repository",
            "https://example.com/project-a.git",
            "--dependency",
            "auth@1.4.2",
        ]),
        Args {
            command: Command::Packages {
                command: PackagesSubcommand::Consumers { .. }
            },
            ..
        }
    ));
    assert!(matches!(
        Args::parse_from(["vm", "packages", "show", "auth"]).command,
        Command::Packages { command: PackagesSubcommand::Show { name } } if name == "auth"
    ));
    assert!(matches!(
        Args::parse_from(["vm", "packages", "consumers", "show", "app"]).command,
        Command::Packages { command: PackagesSubcommand::Consumers { command: super::PackageConsumerSubcommand::Show { name } } } if name == "app"
    ));
    assert!(matches!(
        Args::parse_from(["vm", "packages", "consumers", "drift", "--package", "auth"]).command,
        Command::Packages { command: PackagesSubcommand::Consumers { command: super::PackageConsumerSubcommand::Drift { package: Some(name) } } } if name == "auth"
    ));
}

#[test]
fn tool_refresh_status_and_batch_update_commands_parse() {
    assert!(matches!(
        Args::parse_from(["vm", "tools", "activation-worker", "--once"]).command,
        Command::Tools {
            command: ToolsSubcommand::ActivationWorker { once: true }
        }
    ));
    assert!(matches!(
        Args::parse_from(["vm", "tools", "reconcile-worker", "backend"]).command,
        Command::Tools {
            command: ToolsSubcommand::ReconcileWorker { environment }
        } if environment == "backend"
    ));
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
        Args::parse_from(["vm", "tools", "status", "--env", "backend"]).command,
        Command::Tools {
            command: ToolsSubcommand::Status {
                env: Some(environment)
            }
        } if environment == "backend"
    ));
    assert!(matches!(
        Args::parse_from(["vm", "tools", "enable", "codeatlas", "typemill"]).command,
        Command::Tools {
            command: ToolsSubcommand::Enable { tools }
        } if tools == ["codeatlas", "typemill"]
    ));
    assert!(matches!(
        Args::parse_from(["vm", "tools", "disable", "typemill"]).command,
        Command::Tools {
            command: ToolsSubcommand::Disable { tools }
        } if tools == ["typemill"]
    ));
    assert!(Args::try_parse_from(["vm", "tools", "enable"]).is_err());
    assert!(matches!(
        Args::parse_from(["vm", "tools", "update", "agent-skills", "--background"]).command,
        Command::Tools {
            command: ToolsSubcommand::Update {
                tools,
                env,
                include_stopped: false,
                background: true,
                ..
            }
        } if tools == ["agent-skills"] && env.is_empty()
    ));
    assert!(matches!(
        Args::parse_from(["vm", "tools", "update"]).command,
        Command::Tools {
            command: ToolsSubcommand::Update {
                tools,
                env,
                include_stopped: false,
                ..
            }
        } if tools.is_empty() && env.is_empty()
    ));
    assert!(matches!(
        Args::parse_from([
            "vm",
            "tools",
            "update",
            "agent-skills",
            "helper",
            "--env",
            "backend",
            "--env",
            "worker",
        ])
        .command,
        Command::Tools {
            command: ToolsSubcommand::Update {
                tools,
                env,
                include_stopped: false,
                background: false,
                ..
            }
        } if tools == ["agent-skills", "helper"] && env == ["backend", "worker"]
    ));
    assert!(matches!(
        Args::parse_from(["vm", "tools", "update", "--env", "backend", "agent-skills"])
            .command,
        Command::Tools {
            command: ToolsSubcommand::Update { tools, env, .. }
        } if tools == ["agent-skills"] && env == ["backend"]
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
    assert!(Args::try_parse_from(["vm", "tools", "update", "--fleet"]).is_err());
    assert!(Args::try_parse_from(["vm", "tools", "update", "--all"]).is_err());
    assert!(Args::try_parse_from(["vm", "tools", "update", "--env", "dev", "--all-envs"]).is_err());
    assert!(matches!(
        Args::parse_from(["vm", "tools", "update", "--all-envs"]).command,
        Command::Tools {
            command: ToolsSubcommand::Update { all_envs: true, .. }
        }
    ));
}

#[test]
fn plugin_install_parses() {
    assert!(matches!(
        Args::parse_from(["vm", "plugins", "install", "/path/to/plugin"]).command,
        Command::Plugins {
            command: PluginSubcommand::Install { source_path }
        } if source_path == "/path/to/plugin"
    ));
}

#[test]
fn plugin_new_accepts_only_supported_definition_types() {
    assert!(matches!(
        Args::parse_from(["vm", "plugins", "create", "demo", "--kind", "preset"]).command,
        Command::Plugins {
            command: PluginSubcommand::Create { plugin_name, kind }
        } if plugin_name == "demo" && kind == "preset"
    ));
    assert!(
        Args::try_parse_from(["vm", "plugins", "create", "demo", "--kind", "command"]).is_err()
    );
}

#[test]
fn db_remains_top_level_builtin_command() {
    assert!(matches!(
        Args::parse_from(["vm", "db", "list"]).command,
        Command::Db {
            command: DbSubcommand::List
        }
    ));
}

#[test]
fn db_backups_require_an_explicit_target() {
    assert!(Args::try_parse_from(["vm", "db", "backups", "create", "daily"]).is_err());
    assert!(Args::try_parse_from([
        "vm",
        "db",
        "backups",
        "create",
        "daily",
        "--database",
        "app"
    ])
    .is_ok());
    assert!(Args::try_parse_from([
        "vm",
        "db",
        "backups",
        "create",
        "daily",
        "--all",
        "--database",
        "app"
    ])
    .is_err());
    assert!(Args::try_parse_from(["vm", "db", "backups", "restore", "daily.dump"]).is_err());
}

#[test]
fn named_tunnel_requires_both_endpoints() {
    assert!(Args::try_parse_from([
        "vm",
        "tunnels",
        "open",
        "app",
        "--local",
        "localhost:8080",
        "--remote",
        "localhost:3000"
    ])
    .is_ok());
    assert!(Args::try_parse_from(["vm", "tunnels", "open", "app"]).is_err());
}

#[test]
fn secrets_require_explicit_secure_input_or_a_prompt() {
    assert!(matches!(
        Args::parse_from(["vm", "secrets", "set", "TOKEN", "--stdin"]).command,
        Command::Secrets {
            command: super::SecretSubcommand::Set {
                name,
                stdin: true,
                file: None,
                ..
            }
        } if name == "TOKEN"
    ));
    assert!(Args::try_parse_from(["vm", "secrets", "set", "TOKEN", "value"]).is_err());
    assert!(Args::try_parse_from(["vm", "secrets", "show", "TOKEN"]).is_err());
}

#[test]
fn config_render_parses_environment() {
    assert!(matches!(
        Args::parse_from(["vm", "config", "render", "--env", "feature"]).command,
        Command::Config {
            command: ConfigSubcommand::Render {
                env: Some(env)
            }
        } if env == "feature"
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
