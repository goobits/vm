// Standard library
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// External crates
use anyhow::{Context, Result};
use vm_common::vm_success;

// Internal imports
use crate::link_detector::LinkDetector;
use crate::package_manager::PackageManager;

// Path constants for user directories
const CARGO_HOME_PATH: &str = ".cargo";
const NVM_DIR_PATH: &str = ".nvm";
const LOCAL_BIN_PATH: &str = ".local/bin";

/// Validate a filename for script creation (no path separators, safe characters only)
fn validate_script_name(filename: &str) -> Result<()> {
    // Check for empty name
    if filename.is_empty() {
        anyhow::bail!("Script name cannot be empty");
    }

    // Check for path separators
    if filename.contains('/') || filename.contains('\\') {
        anyhow::bail!("Script name cannot contain path separators: {}", filename);
    }

    // Check for dangerous characters
    if filename.contains("..") || filename.starts_with('.') {
        anyhow::bail!(
            "Script name cannot contain '..' or start with '.': {}",
            filename
        );
    }

    // Only allow alphanumeric, dash, underscore
    if !filename
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        anyhow::bail!(
            "Script name can only contain alphanumeric characters, dashes, and underscores: {}",
            filename
        );
    }

    Ok(())
}

pub struct PackageInstaller {
    user: String,
    detector: LinkDetector,
}

impl PackageInstaller {
    pub fn new(user: String) -> Self {
        let detector = LinkDetector::new(user.clone());
        Self { user, detector }
    }

    /// Install a package
    pub fn install(
        &self,
        package: &str,
        manager: PackageManager,
        force_registry: bool,
    ) -> Result<()> {
        // Check if package manager is available
        if !manager.is_available() {
            anyhow::bail!("Package manager {} is not available", manager);
        }

        // Check for linked package first (unless forcing registry install)
        if !force_registry {
            if let Some(linked_path) = self.detector.get_linked_path(package, manager) {
                println!("🔗 Found linked local package: {}", package);
                return self.install_linked(package, manager, &linked_path);
            }
        }

        // Install from registry
        println!(
            "📦 Installing {} package from registry: {}",
            manager, package
        );
        self.install_from_registry(package, manager)
    }

    /// Install a linked package
    fn install_linked(&self, package: &str, manager: PackageManager, path: &Path) -> Result<()> {
        match manager {
            PackageManager::Cargo => self.install_cargo_linked(package, path),
            PackageManager::Npm => self.install_npm_linked(package, path),
            PackageManager::Pip => self.install_pip_linked(package, path),
        }
    }

    /// Install a package from registry
    fn install_from_registry(&self, package: &str, manager: PackageManager) -> Result<()> {
        match manager {
            PackageManager::Cargo => self.install_cargo_registry(package),
            PackageManager::Npm => self.install_npm_registry(package),
            PackageManager::Pip => self.install_pip_registry(package),
        }
    }

    // === Cargo Implementation ===

    fn install_cargo_linked(&self, package: &str, path: &Path) -> Result<()> {
        println!("  -> Installing local cargo package from: {:?}", path);

        let mut cmd = Command::new("cargo");
        cmd.arg("install").arg("--path").arg(path);

        // Set CARGO_HOME if needed
        let cargo_home = format!("/home/{}/{}", self.user, CARGO_HOME_PATH);
        cmd.env("CARGO_HOME", &cargo_home);

        let status = cmd.status().context("Failed to execute cargo install")?;

        if !status.success() {
            anyhow::bail!("Cargo install failed for linked package: {}", package);
        }

        vm_success!("Installed linked cargo package: {}", package);
        Ok(())
    }

    fn install_cargo_registry(&self, package: &str) -> Result<()> {
        let mut cmd = Command::new("cargo");
        cmd.arg("install").arg(package);

        let cargo_home = format!("/home/{}/{}", self.user, CARGO_HOME_PATH);
        cmd.env("CARGO_HOME", &cargo_home);

        let status = cmd.status().context("Failed to execute cargo install")?;

        if !status.success() {
            anyhow::bail!("Cargo install failed for package: {}", package);
        }

        vm_success!("Installed cargo package from registry: {}", package);
        Ok(())
    }

    // === NPM Implementation ===

    fn install_npm_linked(&self, package: &str, path: &Path) -> Result<()> {
        println!("  -> Linking local npm package from: {:?}", path);

        // Change to package directory first
        let mut cmd = Command::new("npm");
        cmd.arg("link");
        cmd.current_dir(path);

        // Set NVM_DIR if needed
        let nvm_dir = format!("/home/{}/{}", self.user, NVM_DIR_PATH);
        cmd.env("NVM_DIR", &nvm_dir);

        let status = cmd.status().context("Failed to execute npm link")?;

        if !status.success() {
            anyhow::bail!("NPM link failed for package: {}", package);
        }

        vm_success!("Linked npm package: {}", package);
        Ok(())
    }

    fn install_npm_registry(&self, package: &str) -> Result<()> {
        let mut cmd = Command::new("npm");
        cmd.arg("install").arg("-g").arg(package);

        let nvm_dir = format!("/home/{}/{}", self.user, NVM_DIR_PATH);
        cmd.env("NVM_DIR", &nvm_dir);

        let status = cmd.status().context("Failed to execute npm install")?;

        if !status.success() {
            anyhow::bail!("NPM install failed for package: {}", package);
        }

        vm_success!("Installed npm package from registry: {}", package);
        Ok(())
    }

    // === Pip/Pipx Implementation ===

    fn find_pip_executable() -> String {
        // Prefer pip3, then pip, as per Python best practices
        if which::which("pip3").is_ok() {
            "pip3".to_string()
        } else {
            "pip".to_string()
        }
    }

    fn install_pip_linked(&self, package: &str, path: &Path) -> Result<()> {
        // Check if it's a pipx environment
        if LinkDetector::is_pipx_environment(path) {
            println!("  -> Detected as a pipx environment");
            return self.create_pipx_wrappers(package, path);
        }

        // Check if it's a Python project
        if LinkDetector::is_python_project(path) {
            println!("  -> Detected as a Python project, installing in editable mode");
            return Self::install_pip_editable(package, path);
        }

        // Fallback to editable install
        println!("  -> Installing as editable Python package");
        Self::install_pip_editable(package, path)
    }

    fn install_pip_registry(&self, package: &str) -> Result<()> {
        // First try pipx (for CLI tools)
        match Self::try_pipx_install(package) {
            Ok(true) => {
                vm_success!("Installed {} as CLI tool with pipx", package);
                return Ok(());
            }
            Ok(false) => {
                // Pipx indicated it's a library, not a CLI tool
                println!(
                    "📚 {} appears to be a library, installing with pip...",
                    package
                );
            }
            Err(_) => {
                // Pipx not available or other error, try pip
                println!("  -> Pipx not available, using pip");
            }
        }

        // Install with pip
        let pip_exe = Self::find_pip_executable();
        let mut cmd = Command::new(pip_exe);
        cmd.args(["install", "--user", "--break-system-packages", package]);

        let status = cmd.status().context("Failed to execute pip install")?;

        if !status.success() {
            anyhow::bail!("Pip install failed for package: {}", package);
        }

        vm_success!("Installed Python package with pip: {}", package);
        Ok(())
    }

    fn install_pip_editable(_package: &str, path: &Path) -> Result<()> {
        let pip_exe = Self::find_pip_executable();
        let mut cmd = Command::new(pip_exe);
        cmd.args(["install", "--user", "--break-system-packages", "-e"]);
        cmd.arg(path);

        let status = cmd.status().context("Failed to execute pip install")?;

        if !status.success() {
            anyhow::bail!("Pip editable install failed");
        }

        Ok(())
    }

    fn try_pipx_install(package: &str) -> Result<bool> {
        // Check if pipx is available
        if which::which("pipx").is_err() {
            return Ok(false);
        }

        let output = Command::new("pipx")
            .arg("install")
            .arg(package)
            .stderr(Stdio::piped())
            .output()
            .context("Failed to execute pipx")?;

        if output.status.success() {
            return Ok(true);
        }

        // Check if it failed because it's a library
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("No apps associated with package")
            || stderr.contains("not a valid package")
            || stderr.contains("library")
        {
            return Ok(false);
        }

        // Some other error
        anyhow::bail!("Pipx install failed: {}", stderr);
    }

    fn create_pipx_wrappers(&self, package: &str, path: &Path) -> Result<()> {
        let bin_dir = path.join("bin");
        if !bin_dir.exists() {
            println!("  -> No bin directory found in pipx environment");
            return Ok(());
        }

        let local_bin = PathBuf::from(format!("/home/{}/{}", self.user, LOCAL_BIN_PATH));
        fs::create_dir_all(&local_bin)?;

        println!("  -> Creating wrapper scripts in {:?}", local_bin);

        for entry in fs::read_dir(&bin_dir)? {
            let entry = entry?;
            let script_path = entry.path();

            if script_path.is_file() {
                let script_name = script_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .context("Invalid script name")?;

                // Validate script name for security
                validate_script_name(script_name).with_context(|| {
                    format!("Invalid script name from pipx environment: {}", script_name)
                })?;

                let wrapper_path = local_bin.join(script_name);
                Self::create_wrapper_script(&wrapper_path, &script_path, path)?;

                println!("    - Created wrapper: {}", script_name);
            }
        }

        vm_success!("Wrapper scripts created for pipx package: {}", package);
        println!("  -> Please restart your shell to use them");
        Ok(())
    }

    fn create_wrapper_script(
        wrapper_path: &Path,
        script_path: &Path,
        linked_dir: &Path,
    ) -> Result<()> {
        let wrapper_content = format!(
            r#"#!/bin/sh
# VM-Tool generated wrapper for linked pipx package
set -e

LINKED_DIR="{}"
SCRIPT_PATH="{}"

# Find site-packages with multiple strategies
SITE_PACKAGES=""

# Strategy 1: Look for standard python version paths
for pydir in "$LINKED_DIR"/lib/python*/site-packages; do
    if [ -d "$pydir" ]; then
        SITE_PACKAGES="$pydir"
        break
    fi
done

# Strategy 2: Use find as fallback
if [ -z "$SITE_PACKAGES" ]; then
    SITE_PACKAGES=$(find "$LINKED_DIR" -type d -name "site-packages" 2>/dev/null | head -1)
fi

# Strategy 3: Check if there's a venv structure
if [ -z "$SITE_PACKAGES" ] && [ -d "$LINKED_DIR/lib" ]; then
    if [ -d "$LINKED_DIR/lib/site-packages" ]; then
        SITE_PACKAGES="$LINKED_DIR/lib/site-packages"
    fi
fi

# Export PYTHONPATH if we found site-packages
if [ -n "$SITE_PACKAGES" ]; then
    export PYTHONPATH="$SITE_PACKAGES:${{PYTHONPATH:-}}"
    export PYTHONPATH="$LINKED_DIR:$PYTHONPATH"
fi

# Execute the script with python3
exec python3 "$SCRIPT_PATH" "$@"
"#,
            linked_dir.display(),
            script_path.display()
        );

        let mut file = fs::File::create(wrapper_path)?;
        file.write_all(wrapper_content.as_bytes())?;

        // Make executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(wrapper_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(wrapper_path, perms)?;
        }

        Ok(())
    }

    // === Helper methods ===

    pub fn is_linked(&self, package: &str, manager: PackageManager) -> Result<bool> {
        self.detector.is_linked(package, manager)
    }

    pub fn list_linked(&self, manager: Option<PackageManager>) {
        let linked = self.detector.list_linked(manager);

        if linked.is_empty() {
            println!("No linked packages found");
            return;
        }

        println!("🔗 Linked packages:");
        let mut current_manager = None;

        for (mgr, package) in linked {
            if current_manager != Some(mgr) {
                println!("\n  {}:", mgr);
                current_manager = Some(mgr);
            }
            println!("    - {}", package);
        }
    }
}
