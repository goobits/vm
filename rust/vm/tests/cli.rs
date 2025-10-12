// Entrypoint for CLI-related integration tests.

#[path = "cli"]
mod cli {
    pub mod config_commands;
    pub mod pkg_commands;
}
