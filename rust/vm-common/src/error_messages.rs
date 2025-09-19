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

/// Helper to combine error messages with suggestions
pub fn error_with_suggestion(message: &str, suggestion: &str) -> String {
    format!("{message}\n💡 {suggestion}")
}
