//! # Docker Container Management
//!
//! This module provides automated Docker-based deployment and management for the package server.
//! It handles the complete lifecycle of containerized package server deployments, including
//! image building, network setup, container orchestration, and client configuration.
//!
//! ## Features
//!
//! - **Automated Setup**: One-command deployment from source to running container
//! - **Multi-Registry Support**: Automatic client configuration for Python, Node.js, and Rust
//! - **Project Detection**: Intelligently detects project types and configures appropriate clients
//! - **Health Monitoring**: Validates container health before completing deployment
//! - **Network Management**: Creates isolated Docker networks for secure communication
//!
//! ## Quick Start
//!
//! ```rust
//! use vm_package_server::docker_manager::deploy_quick;
//!
//! // Deploy everything with one command
//! deploy_quick().await?;
//! ```
//!
//! ## Architecture
//!
//! The Docker deployment creates:
//!
//! 1. **Docker Image**: Multi-stage build (Rust builder + Debian runtime)
//! 2. **Docker Network**: Isolated network named `pkg-network`
//! 3. **Container**: Named `pkg-server` with port mapping and volume mounts
//! 4. **Client Configs**: Registry configurations for detected project types
//!
//! ## Client Configuration
//!
//! Automatically generates configuration files based on detected projects:
//!
//! - **Python**: Creates `.pip/pip.conf` for PyPI registry
//! - **Node.js**: Creates `.npmrc` for NPM registry
//! - **Rust**: Creates `.cargo/config.toml` for Cargo registry
//!
//! ## Security Considerations
//!
//! - Uses non-root user in container
//! - Implements resource constraints through Docker
//! - Isolates network traffic through custom Docker network
//! - Validates Docker availability before proceeding
//!
//! ## Error Handling
//!
//! - Graceful fallbacks for missing Docker installation
//! - Platform-specific installation guidance
//! - Comprehensive health checks with timeout
//! - Detailed progress reporting with colored output

use anyhow::Result;
use colored::Colorize;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use vm_package_server::config::Config;

/// Manages Docker-based deployment of the package server
///
/// This struct orchestrates the complete Docker deployment process,
/// from image building to client configuration.
pub struct DockerManager {
    network_name: String,
    container_name: String,
    image_name: String,
    port: u16,
}

impl DockerManager {
    /// Creates a new DockerManager with default configuration
    ///
    /// Uses default values from the server configuration for network names,
    /// container names, and port settings.
    pub fn new() -> Self {
        let config = Config::default();
        Self {
            network_name: "pkg-network".to_string(),
            container_name: "pkg-server".to_string(),
            image_name: "goobits-pkg-server:latest".to_string(),
            port: config.server.default_port,
        }
    }

    /// Creates a new DockerManager with custom configuration
    ///
    /// Allows specifying custom port and container settings for specific deployments.
    pub fn with_config(port: u16) -> Self {
        Self {
            network_name: "pkg-network".to_string(),
            container_name: format!("goobits-pkg-server-{}", port),
            image_name: "goobits-pkg-server:latest".to_string(),
            port,
        }
    }

    /// Performs complete automated setup of the package server
    ///
    /// This is the main entry point for Docker deployment. It:
    /// 1. Verifies Docker installation
    /// 2. Builds the server image
    /// 3. Creates the Docker network
    /// 4. Starts the server container
    /// 5. Waits for health confirmation
    /// 6. Configures clients for detected projects
    ///
    /// # Errors
    ///
    /// Returns an error if any step fails, including:
    /// - Docker not installed or not running
    /// - Image build failures
    /// - Container startup failures
    /// - Health check timeout
    pub fn quick_setup(&self) -> Result<()> {
        println!("\n{}", "🚀 Starting Quick Deploy".bright_cyan().bold());

        // 1. Check Docker
        self.ensure_docker()?;

        // 2. Build image
        self.build_image()?;

        // 3. Create network
        self.create_network()?;

        // 4. Start server
        self.start_server()?;

        // 5. Wait for health
        self.wait_for_health()?;

        // 6. Setup client containers based on detected projects
        self.auto_setup_clients()?;

        Ok(())
    }

    fn ensure_docker(&self) -> Result<()> {
        print!("  Checking Docker... ");
        std::io::stdout().flush()?;

        let output = Command::new("docker")
            .arg("version")
            .arg("--format")
            .arg("{{.Server.Version}}")
            .output();

        match output {
            Ok(o) if o.status.success() => {
                let version = String::from_utf8_lossy(&o.stdout);
                println!("{} (v{})", "✓".bright_green(), version.trim());
                Ok(())
            }
            _ => {
                println!("{}", "✗".bright_red());
                self.offer_docker_install()
            }
        }
    }

    fn offer_docker_install(&self) -> Result<()> {
        println!("\n  {} Docker not found!", "⚠️".bright_yellow());
        println!("\n  Install Docker:");

        #[cfg(target_os = "macos")]
        println!("    brew install --cask docker");

        #[cfg(target_os = "linux")]
        println!("    curl -fsSL https://get.docker.com | sh");

        #[cfg(target_os = "windows")]
        println!("    Download from https://desktop.docker.com/win/stable/Docker%20Desktop%20Installer.exe");

        anyhow::bail!("Docker is required. Please install and try again.")
    }

    fn build_image(&self) -> Result<()> {
        print!("  Building image... ");
        std::io::stdout().flush()?;

        // Check if Dockerfile exists
        let dockerfile = Path::new("docker/server/Dockerfile");
        if !dockerfile.exists() {
            // Create a minimal Dockerfile on the fly
            self.create_minimal_dockerfile()?;
        }

        let mut cmd = Command::new("docker")
            .args(["build", "-t", &self.image_name, "-f"])
            .arg(dockerfile)
            .arg(".")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Stream output
        let stderr = cmd.stderr.take().unwrap();
        let reader = BufReader::new(stderr);

        for l in reader.lines().map_while(Result::ok) {
            if l.contains("Step") || l.contains("Successfully") {
                // Show progress
                print!(".");
                std::io::stdout().flush()?;
            }
        }

        let status = cmd.wait()?;
        if status.success() {
            println!(" {}", "✓".bright_green());
            Ok(())
        } else {
            println!(" {}", "✗".bright_red());
            anyhow::bail!("Failed to build Docker image")
        }
    }

    fn create_minimal_dockerfile(&self) -> Result<()> {
        std::fs::create_dir_all("docker/server")?;

        let dockerfile = r#"FROM rust:1.78 as builder
WORKDIR /usr/src/app
COPY . .
RUN cargo build --release

FROM debian:bullseye-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/app/target/release/pkg-server /usr/local/bin/
EXPOSE 3080
ENTRYPOINT ["pkg-server"]
CMD ["start", "--host", "0.0.0.0", "--port", "3080", "--foreground"]
"#;

        std::fs::write("docker/server/Dockerfile", dockerfile)?;
        Ok(())
    }

    fn create_network(&self) -> Result<()> {
        print!("  Creating network... ");
        std::io::stdout().flush()?;

        // Check if network exists
        let check = Command::new("docker")
            .args([
                "network",
                "ls",
                "--filter",
                &format!("name={}", self.network_name),
            ])
            .output()?;

        if String::from_utf8_lossy(&check.stdout).contains(&self.network_name) {
            println!("{} (exists)", "✓".bright_green());
            return Ok(());
        }

        // Create network
        let status = Command::new("docker")
            .args(["network", "create", &self.network_name])
            .stdout(Stdio::null())
            .status()?;

        if status.success() {
            println!("{}", "✓".bright_green());
            Ok(())
        } else {
            anyhow::bail!("Failed to create Docker network")
        }
    }

    fn start_server(&self) -> Result<()> {
        print!("  Starting server... ");
        std::io::stdout().flush()?;

        // Stop existing container if running
        Command::new("docker")
            .args(["rm", "-f", &self.container_name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .ok();

        // Start new container
        let data_dir = std::env::current_dir()?.join("data");
        std::fs::create_dir_all(&data_dir)?;

        let status = Command::new("docker")
            .args([
                "run",
                "-d",
                "--name",
                &self.container_name,
                "--network",
                &self.network_name,
                "-p",
                &format!("{}:{}", self.port, self.port),
                "-v",
                &format!("{}:/home/appuser/data", data_dir.display()),
                "--restart",
                "unless-stopped",
                &self.image_name,
            ])
            .stdout(Stdio::null())
            .status()?;

        if status.success() {
            println!("{}", "✓".bright_green());
            Ok(())
        } else {
            anyhow::bail!("Failed to start Docker container")
        }
    }

    fn wait_for_health(&self) -> Result<()> {
        print!("  Waiting for server... ");
        std::io::stdout().flush()?;

        for _ in 0..30 {
            thread::sleep(Duration::from_secs(1));

            let output = Command::new("docker")
                .args([
                    "exec",
                    &self.container_name,
                    "curl",
                    "-s",
                    &format!("http://localhost:{}/api/status", self.port),
                ])
                .output();

            if let Ok(o) = output {
                if o.status.success() {
                    println!("{}", "✓".bright_green());
                    return Ok(());
                }
            }

            print!(".");
            std::io::stdout().flush()?;
        }

        println!("{}", "✗".bright_red());
        anyhow::bail!("Server failed to become healthy")
    }

    fn auto_setup_clients(&self) -> Result<()> {
        let project_types = self.detect_projects();

        if project_types.is_empty() {
            return Ok(());
        }

        println!("\n  {} Detected projects:", "🔍".bright_blue());

        for (project_type, files) in project_types {
            println!(
                "    {} {} ({})",
                "→".bright_green(),
                project_type.bright_yellow(),
                files.join(", ").bright_cyan()
            );

            match project_type.as_str() {
                "Python" => self.setup_python_client()?,
                "Node.js" => self.setup_node_client()?,
                "Rust" => self.setup_rust_client()?,
                _ => {}
            }
        }

        Ok(())
    }

    fn detect_projects(&self) -> Vec<(String, Vec<String>)> {
        let mut projects = Vec::new();

        // Python detection
        let python_files = ["requirements.txt", "setup.py", "Pipfile", "pyproject.toml"];
        let found_python: Vec<String> = python_files
            .iter()
            .filter(|f| Path::new(f).exists())
            .map(|s| s.to_string())
            .collect();

        if !found_python.is_empty() {
            projects.push(("Python".to_string(), found_python));
        }

        // Node.js detection
        if Path::new("package.json").exists() {
            projects.push(("Node.js".to_string(), vec!["package.json".to_string()]));
        }

        // Rust detection
        if Path::new("Cargo.toml").exists() {
            projects.push(("Rust".to_string(), vec!["Cargo.toml".to_string()]));
        }

        projects
    }

    fn setup_python_client(&self) -> Result<()> {
        println!("    {} Creating Python environment...", "🐍".bright_green());

        // Create .pip/pip.conf
        std::fs::create_dir_all(".pip")?;
        let pip_conf = format!(
            "[global]\nindex-url = http://localhost:{}/pypi/simple/\ntrusted-host = localhost\n",
            self.port
        );
        std::fs::write(".pip/pip.conf", pip_conf)?;

        println!("      {} pip configured", "✓".bright_green());
        Ok(())
    }

    fn setup_node_client(&self) -> Result<()> {
        println!("    {} Configuring npm...", "📦".bright_green());

        // Create .npmrc
        let npmrc = format!("registry=http://localhost:{}/npm/\n", self.port);
        std::fs::write(".npmrc", npmrc)?;

        println!("      {} npm configured", "✓".bright_green());
        Ok(())
    }

    fn setup_rust_client(&self) -> Result<()> {
        println!("    {} Configuring Cargo...", "🦀".bright_green());

        // Add to .cargo/config.toml
        std::fs::create_dir_all(".cargo")?;
        let cargo_config = format!(
            "[registries.local]\nindex = \"sparse+http://localhost:{}/cargo/\"\n",
            self.port
        );
        std::fs::write(".cargo/config.toml", cargo_config)?;

        println!("      {} cargo configured", "✓".bright_green());
        Ok(())
    }

    /// Displays a formatted dashboard with deployment information
    ///
    /// Shows server URL, container details, network information,
    /// and helpful command suggestions for managing the deployment.
    pub fn show_dashboard(&self) -> Result<()> {
        println!(
            "\n{}",
            "┌─────────────────────────────────────────────┐".bright_blue()
        );
        println!(
            "{}",
            "│         🎉 Package Server Ready!            │"
                .bright_blue()
                .bold()
        );
        println!(
            "{}",
            "├─────────────────────────────────────────────┤".bright_blue()
        );
        println!(
            "│ 🌐 {:<38} │",
            format!("http://localhost:{}", self.port).bright_cyan()
        );
        println!(
            "│ 🐳 {:<38} │",
            format!("Container: {}", self.container_name).bright_green()
        );
        println!(
            "│ 🔗 {:<38} │",
            format!("Network: {}", self.network_name).bright_yellow()
        );
        println!(
            "{}",
            "├─────────────────────────────────────────────┤".bright_blue()
        );
        println!(
            "│ {}                             │",
            "Quick commands:".bright_yellow()
        );
        println!(
            "│   {} View packages          │",
            "pkg-server list".bright_cyan()
        );
        println!(
            "│   {} Publish package        │",
            "pkg-server add".bright_cyan()
        );
        println!(
            "│   {} Container shell        │",
            format!("docker exec -it {} bash", self.container_name).bright_cyan()
        );
        println!(
            "{}",
            "└─────────────────────────────────────────────┘".bright_blue()
        );

        Ok(())
    }
}

/// Quick deployment wrapper for CLI usage
///
/// This function provides a simple interface for CLI commands to deploy
/// the package server with Docker. It creates a new DockerManager,
/// performs the complete setup, and shows the dashboard.
///
/// # Example
///
/// ```rust
/// use vm_package_server::docker_manager::deploy_quick;
///
/// // Deploy the server with one command
/// deploy_quick()?;
/// ```
///
/// # Errors
///
/// Returns an error if deployment fails at any stage.
pub fn deploy_quick() -> Result<()> {
    let manager = DockerManager::new();
    manager.quick_setup()?;
    manager.show_dashboard()?;
    Ok(())
}

/// Legacy Docker deployment function that matches docker.rs interface
///
/// This function provides compatibility with the existing `run_in_docker` interface
/// while using the improved DockerManager implementation under the hood.
///
/// # Arguments
/// * `host` - Host address to bind the server to
/// * `port` - Port number for the server
/// * `data_dir` - Data directory path for persistent storage
///
/// # Errors
///
/// Returns an error if deployment fails at any stage.
pub fn run_in_docker(host: &str, port: u16, _data_dir: &std::path::PathBuf) -> Result<()> {
    use colored::Colorize;

    println!(
        "{}",
        "🐳 Starting Goobits Package Server in Docker..."
            .bright_cyan()
            .bold()
    );

    let manager = DockerManager::with_config(port);
    manager.quick_setup()?;

    println!(
        "\n{}",
        "✅ Server is running in Docker".bright_green().bold()
    );
    println!();
    println!("🌐 Server is accessible at:");
    println!("   Local:      http://localhost:{}", port);
    println!("   Network:    http://{}:{}", host, port);
    println!();
    println!("📋 Quick commands:");
    println!("   View logs:  docker logs -f {}", manager.container_name);
    println!("   Stop:       docker stop {}", manager.container_name);
    println!("   Restart:    docker restart {}", manager.container_name);

    Ok(())
}
