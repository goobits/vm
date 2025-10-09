use clap::Subcommand;

/// Port management subcommands
#[derive(Subcommand)]
#[command(verbatim_doc_comment)]
pub enum PortsCmd {
    /// Check for port range conflicts
    Check {
        /// Port range (e.g., "3000-3009")
        range: String,
        /// Optional project name to exclude from conflict checking
        project_name: Option<String>,
    },
    /// Register a port range for a project
    Register {
        /// Port range (e.g., "3000-3009")
        range: String,
        /// Project name
        project: String,
        /// Project path
        path: String,
    },
    /// Suggest next available port range
    Suggest {
        /// Range size (default: 10)
        size: Option<u16>,
    },
    /// List all registered port ranges
    List,
    /// Unregister a project's port range
    Unregister {
        /// Project name
        project: String,
    },
}
