// Entrypoint for CLI-related integration tests.

#[cfg(feature = "integration")]
#[path = "cli"]
mod cli {
    pub mod config_commands;
    pub mod pkg_commands;
}
