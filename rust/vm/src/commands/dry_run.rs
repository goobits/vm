use crate::cli::{Args, Command};
use vm_core::vm_println;

pub(super) fn print_summary(args: &Args) {
    vm_println!("Dry run: would {}", dry_run_description(&args.command));
    if let Some(config) = &args.config {
        vm_println!("Config: {}", config.display());
    }
    vm_println!("No changes made.");
}

fn dry_run_description(command: &Command) -> String {
    let target = |environment: &Option<String>| {
        environment
            .as_deref()
            .unwrap_or("the default environment")
            .to_string()
    };

    match command {
        Command::Create { environment, force } => format!(
            "{} {}",
            if *force { "recreate" } else { "create" },
            target(environment)
        ),
        Command::Start {
            environment, fleet, ..
        } => {
            if fleet.fleet {
                "start matching managed environments".to_string()
            } else {
                format!("start {}", target(environment))
            }
        }
        Command::Run { kind, words, .. } => {
            let name = words
                .last()
                .filter(|_| words.len() == 2)
                .map_or(String::new(), |name| format!(" as {name}"));
            format!("run {}{name}", format!("{kind:?}").to_ascii_lowercase())
        }
        Command::List { all, .. } => {
            format!("list {} environments", if *all { "all" } else { "project" })
        }
        Command::Shell {
            environment,
            command,
            ..
        } => format!(
            "{} {}",
            if command.is_some() {
                "run a shell command in"
            } else {
                "open a shell in"
            },
            target(environment)
        ),
        Command::Exec {
            environment, fleet, ..
        } => {
            if fleet.fleet {
                "execute a command in matching managed environments".to_string()
            } else {
                format!("execute a command in {}", target(environment))
            }
        }
        Command::Logs { environment, .. } => format!("read logs from {}", target(environment)),
        Command::Copy { fleet, .. } => {
            if fleet.fleet {
                "copy files across matching managed environments".to_string()
            } else {
                "copy files without changing lifecycle state".to_string()
            }
        }
        Command::Stop {
            environment, fleet, ..
        } => {
            if fleet.fleet {
                "stop matching managed environments".to_string()
            } else {
                format!("stop {}", target(environment))
            }
        }
        Command::Status { environment } => format!("inspect {}", target(environment)),
        Command::Restart {
            environment, fleet, ..
        } => {
            if fleet.fleet {
                "restart matching managed environments".to_string()
            } else {
                format!("restart {}", target(environment))
            }
        }
        Command::Remove { environment, .. } => format!("remove {}", target(environment)),
        Command::Save { .. } => "save an environment snapshot".to_string(),
        Command::Revert { .. } => "restore an environment snapshot".to_string(),
        Command::Package { environment, .. } => format!("package {}", target(environment)),
        Command::Packages {
            command: crate::cli::PackagesSubcommand::Open { source },
        } => format!("open the original workspace for '{source}' in its owning Docker environment"),
        Command::Packages { .. } => "manage package infrastructure".to_string(),
        Command::Tools { .. } => "manage guest tools".to_string(),
        Command::Config { .. } => "run a configuration operation".to_string(),
        Command::Tunnel { .. } => "run a tunnel operation".to_string(),
        Command::Doctor { .. } => "run diagnostics or requested maintenance".to_string(),
        Command::Plugin { .. } => "run a plugin operation".to_string(),
        Command::System { .. } => "run a system operation".to_string(),
        Command::Db { .. } => "run a database operation".to_string(),
        Command::Secret { .. } => "run a secret operation (values redacted)".to_string(),
        Command::InternalCompletion { .. } => "generate shell completions".to_string(),
        Command::GetSyncDirectory => "print the workspace directory".to_string(),
    }
}
