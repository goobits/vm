//! Common error messages and suggestions for the VM management application

/// Common error messages that can be reused across the application
pub mod messages {
    pub const DOCKER_NOT_RUNNING: &str = "Docker daemon is not running";
    pub const DOCKER_NOT_INSTALLED: &str = "Docker is not installed or not in PATH";
    pub const CONFIG_NOT_FOUND: &str = "Configuration file not found";
    pub const CONTAINER_ALREADY_EXISTS: &str = "Container already exists";
    pub const INSUFFICIENT_PERMISSIONS: &str = "Insufficient permissions";
    pub const PORT_IN_USE: &str = "Port is already in use";
    pub const DISK_SPACE_LOW: &str = "Insufficient disk space";
}

/// Container state messages
pub mod container_states {
    pub const NOT_RUNNING: &str = "🔴 VM is stopped";
    pub const NOT_FOUND: &str = "🔍 VM doesn't exist";
    pub const ALREADY_RUNNING: &str = "✅ VM is already running";
    pub const STARTING: &str = "🟡 VM is starting up";
    pub const UNHEALTHY: &str = "⚠️ VM is unhealthy";
}

/// Command-specific messages
pub mod command_messages {
    pub const SSH_FAILED: &str = "🔌 Cannot connect to VM";
    pub const CREATE_FAILED: &str = "❌ Failed to create VM";
    pub const START_FAILED: &str = "❌ Failed to start VM";
    pub const STOP_FAILED: &str = "❌ Failed to stop VM";
    pub const DESTROY_FAILED: &str = "❌ Failed to destroy VM";
    pub const EXEC_FAILED: &str = "⚠️ Command execution failed";
    pub const PROVISION_FAILED: &str = "❌ Provisioning failed";
}

/// Common suggestions for fixing errors
pub mod suggestions {
    pub const START_DOCKER: &str = "Start Docker with: sudo systemctl start docker";
    pub const START_DOCKER_DESKTOP: &str = "Start Docker Desktop application";
    pub const INSTALL_DOCKER: &str = "Install Docker from: https://docs.docker.com/get-docker/";
    pub const CREATE_CONFIG: &str = "Create configuration with: vm init";
    pub const CHECK_PERMISSIONS: &str = "Add user to docker group: sudo usermod -aG docker $USER";
    pub const FREE_DISK_SPACE: &str = "Free up disk space or use docker system prune";
    pub const CHECK_PORT_USAGE: &str = "Check what's using the port: lsof -i :PORT";
}

/// Command-specific suggestions
pub mod command_suggestions {
    pub const START_VM: &str = "🚀 Start with: vm start";
    pub const CREATE_VM: &str = "💡 Create with: vm create";
    pub const USE_SSH: &str = "🔌 Connect with: vm ssh";
    pub const CHECK_STATUS: &str = "📊 Check status: vm status";
    pub const VIEW_LOGS: &str = "📋 View logs: vm logs";
    pub const RESTART_VM: &str = "🔄 Restart with: vm restart";
    pub const USE_FORCE: &str = "⚡ Force with: vm create --force";
}

/// Helper to combine error messages with suggestions
pub fn error_with_suggestion(message: &str, suggestion: &str) -> String {
    format!("{message}\n💡 {suggestion}")
}
