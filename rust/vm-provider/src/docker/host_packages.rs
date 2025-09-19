// Host package detection for all package managers
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use vm_common::{vm_error_hint, vm_warning};

#[derive(Debug, Clone)]
pub enum PackageLocation {
    HostPip(PathBuf),
    HostPipx(PathBuf),
    HostNpm(PathBuf),
    HostCargo(PathBuf),
    NotFound,
}

#[derive(Debug, Clone)]
pub enum PackageManager {
    Pip,
    #[allow(dead_code)]
    Pipx,
    Npm,
    Cargo,
}

#[derive(Debug, Clone)]
pub struct HostPackageInfo {
    // Python package locations
    pub pip_site_packages: Option<PathBuf>,
    pub pipx_base_dir: Option<PathBuf>,
    // NPM package locations
    pub npm_global_dir: Option<PathBuf>,
    pub npm_local_dir: Option<PathBuf>,
    // Cargo package locations
    pub cargo_registry: Option<PathBuf>,
    pub cargo_bin: Option<PathBuf>,
    // Detected packages by manager
    pub detected_packages: HashMap<String, PackageLocation>,
}

impl HostPackageInfo {
    pub fn new() -> Self {
        Self {
            pip_site_packages: None,
            pipx_base_dir: None,
            npm_global_dir: None,
            npm_local_dir: None,
            cargo_registry: None,
            cargo_bin: None,
            detected_packages: HashMap::new(),
        }
    }
}

/// Detects packages for the specified package manager on the host system.
///
/// This function scans the host system for installed packages matching the provided
/// list and returns information about their locations and availability for mounting.
///
/// # Arguments
/// * `packages` - List of package names to search for
/// * `manager` - The package manager to use for detection
///
/// # Returns
/// * `HostPackageInfo` - Information about detected packages and their locations
pub fn detect_packages(packages: &[String], manager: PackageManager) -> HostPackageInfo {
    let mut info = HostPackageInfo::new();

    // Detect package manager directories
    detect_package_directories(&mut info);

    // Check each package based on manager type
    for package in packages {
        let location = match manager {
            PackageManager::Pip | PackageManager::Pipx => detect_python_package(package, &info),
            PackageManager::Npm => detect_npm_package(package, &info),
            PackageManager::Cargo => detect_cargo_package(package, &info),
        };
        info.detected_packages.insert(package.clone(), location);
    }

    info
}

/// Detect all package manager directories on the host
fn detect_package_directories(info: &mut HostPackageInfo) {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());

    // Python directories
    if let Ok(output) = Command::new("python3")
        .args(["-c", "import site; print(site.getusersitepackages())"])
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            info.pip_site_packages = Some(PathBuf::from(path));
        }
    }

    if let Ok(output) = Command::new("pipx")
        .arg("environment")
        .arg("--value")
        .arg("PIPX_HOME")
        .output()
    {
        if output.status.success() {
            let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
            let pipx_venvs = path.join("venvs");
            if pipx_venvs.exists() {
                info.pipx_base_dir = Some(pipx_venvs);
            }
        }
    }

    // NPM directories
    if let Ok(output) = Command::new("npm").args(["root", "-g"]).output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            info.npm_global_dir = Some(PathBuf::from(path));
        }
    }

    // Check for local node_modules (relative to project directory)
    let local_npm = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("node_modules");
    if local_npm.exists() {
        info.npm_local_dir = Some(local_npm);
    }

    // Cargo directories - use platform abstraction for cross-platform paths
    if let Ok(cargo_home) = vm_platform::current().cargo_home() {
        let cargo_registry = cargo_home.join("registry");
        if cargo_registry.exists() {
            info.cargo_registry = Some(cargo_registry);
        }

        let cargo_bin = cargo_home.join("bin");
        if cargo_bin.exists() {
            info.cargo_bin = Some(cargo_bin);
        }
    } else {
        // Fallback to home-based paths if platform detection fails
        let cargo_registry = PathBuf::from(&home).join(".cargo/registry");
        if cargo_registry.exists() {
            info.cargo_registry = Some(cargo_registry);
        }

        let cargo_bin = PathBuf::from(&home).join(".cargo/bin");
        if cargo_bin.exists() {
            info.cargo_bin = Some(cargo_bin);
        }
    }
}

/// Detect Python package (pip or pipx)
fn detect_python_package(package: &str, info: &HostPackageInfo) -> PackageLocation {
    // Check pip first
    if let Some(ref pip_dir) = info.pip_site_packages {
        if check_pip_package(package, pip_dir) {
            return PackageLocation::HostPip(pip_dir.clone());
        }
    }

    // Check pipx
    if let Some(ref pipx_dir) = info.pipx_base_dir {
        if let Some(path) = check_pipx_package(package, pipx_dir) {
            return PackageLocation::HostPipx(path);
        }
    }

    PackageLocation::NotFound
}

/// Detect NPM package (global or local)
fn detect_npm_package(package: &str, info: &HostPackageInfo) -> PackageLocation {
    // Check local node_modules first (project dependencies)
    if let Some(ref local_dir) = info.npm_local_dir {
        let package_path = local_dir.join(package);
        if package_path.exists() {
            return PackageLocation::HostNpm(package_path);
        }
    }

    // Check global npm packages
    if let Some(ref global_dir) = info.npm_global_dir {
        let package_path = global_dir.join(package);
        if package_path.exists() {
            return PackageLocation::HostNpm(package_path);
        }
    }

    // Check using npm list
    if let Ok(output) = Command::new("npm")
        .args(["list", "-g", "--depth=0", package])
        .output()
    {
        if output.status.success() {
            if let Some(ref global_dir) = info.npm_global_dir {
                return PackageLocation::HostNpm(global_dir.clone());
            }
        }
    }

    PackageLocation::NotFound
}

/// Detect Cargo package
fn detect_cargo_package(package: &str, info: &HostPackageInfo) -> PackageLocation {
    // Check cargo bin for installed binaries
    if let Some(ref bin_dir) = info.cargo_bin {
        let binary_path = bin_dir.join(package);
        if binary_path.exists() {
            return PackageLocation::HostCargo(binary_path);
        }
    }

    // Check using cargo
    if let Ok(output) = Command::new("cargo").args(["install", "--list"]).output() {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            if output_str.contains(package) {
                if let Some(ref bin_dir) = info.cargo_bin {
                    return PackageLocation::HostCargo(bin_dir.clone());
                }
            }
        }
    }

    PackageLocation::NotFound
}

/// Check if package exists in pip site-packages
fn check_pip_package(package: &str, site_packages: &Path) -> bool {
    // Check using pip show command
    if let Ok(output) = Command::new("python3")
        .args(["-m", "pip", "show", package])
        .output()
    {
        if output.status.success() {
            return true;
        }
    }

    // Fallback: check directory existence
    let package_dir = site_packages.join(package);
    let package_underscore = site_packages.join(package.replace("-", "_"));

    package_dir.exists() || package_underscore.exists()
}

/// Check if package exists in pipx and return its path
fn check_pipx_package(package: &str, pipx_base: &Path) -> Option<PathBuf> {
    // Check using pipx list
    if let Ok(output) = Command::new("pipx").args(["list", "--short"]).output() {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            if output_str.contains(package) {
                let package_venv = pipx_base.join(package);
                if package_venv.exists() {
                    return Some(package_venv);
                }
            }
        }
    }

    // Fallback: check directory existence
    let package_dir = pipx_base.join(package);
    if package_dir.exists() {
        Some(package_dir)
    } else {
        None
    }
}

/// Check if a path is accessible to Docker on macOS
fn is_docker_accessible(path: &Path) -> bool {
    let path_str = path.to_string_lossy();

    // Default Docker Desktop shared paths on macOS
    let shared_prefixes = [
        "/Users/",
        "/tmp/",
        "/private/tmp/",
        "/private/var/folders/", // Temp directories
    ];

    let is_accessible = shared_prefixes
        .iter()
        .any(|prefix| path_str.starts_with(prefix));
    is_accessible
}

/// Get volume mount specifications for all host packages
pub fn get_volume_mounts(info: &HostPackageInfo) -> Vec<(PathBuf, String)> {
    let mut mounts = Vec::new();
    let mut skipped_paths = Vec::new();
    let mut mounted_base_dirs = std::collections::HashSet::new();

    let mut try_add_mount = |path: &PathBuf, container_path: &str, package_type: &str| {
        if path.exists() {
            if is_docker_accessible(path) {
                mounts.push((path.clone(), container_path.to_string()));
            } else {
                skipped_paths.push((path.clone(), package_type.to_string()));
            }
        }
    };

    // Iterate over the specifically detected packages
    for (package_name, location) in &info.detected_packages {
        match location {
            // For pip, we mount the entire site-packages directory. Do it only once.
            PackageLocation::HostPip(path) => {
                if !mounted_base_dirs.contains(path) {
                    try_add_mount(path, "/host/pip", "pip site-packages");
                    mounted_base_dirs.insert(path.clone());
                }
            }
            // For pipx, mount the specific package directory.
            PackageLocation::HostPipx(path) => {
                let container_path = format!("/host/pipx/{}", package_name);
                try_add_mount(
                    path,
                    &container_path,
                    &format!("pipx package ({})", package_name),
                );
            }
            // For npm, mount the specific package directory.
            PackageLocation::HostNpm(path) => {
                let container_path = format!("/host/npm/{}", package_name);
                try_add_mount(
                    path,
                    &container_path,
                    &format!("npm package ({})", package_name),
                );
            }
            // For cargo, we mount the ~/.cargo/bin directory. Do it only once.
            PackageLocation::HostCargo(path) => {
                if let Some(parent_dir) = path.parent() {
                    if !mounted_base_dirs.contains(parent_dir) {
                        try_add_mount(&parent_dir.to_path_buf(), "/host/cargo/bin", "cargo bin");
                        mounted_base_dirs.insert(parent_dir.to_path_buf());
                    }
                }
            }
            PackageLocation::NotFound => continue,
        }
    }

    // Also handle the generic npm local and cargo registry mounts which are not package-specific.
    if let Some(ref npm_local) = info.npm_local_dir {
        try_add_mount(npm_local, "/host/npm/local", "npm local");
    }
    if let Some(ref cargo_registry) = info.cargo_registry {
        try_add_mount(cargo_registry, "/host/cargo/registry", "cargo registry");
    }

    // Log warnings for skipped paths
    if !skipped_paths.is_empty() {
        vm_warning!("Skipping host package mounts (not shared with Docker):");
        for (path, package_type) in skipped_paths {
            vm_warning!(
                "   {} ({}): Add to Docker Desktop File Sharing to enable",
                package_type,
                path.display()
            );
        }
        vm_error_hint!("To enable: Docker Desktop → Settings → Resources → File Sharing");
    }

    mounts
}

/// Get environment variables for package manager configurations
pub fn get_package_env_vars(info: &HostPackageInfo) -> Vec<(String, String)> {
    let mut env_vars = Vec::new();

    // Python package environment variables
    if info.pip_site_packages.is_some() {
        env_vars.push(("HOST_PIP_PACKAGES".to_string(), "/host/pip".to_string()));
    }

    if info.pipx_base_dir.is_some() {
        env_vars.push(("HOST_PIPX_PACKAGES".to_string(), "/host/pipx".to_string()));
    }

    // NPM package environment variables
    if info.npm_global_dir.is_some() {
        env_vars.push((
            "HOST_NPM_GLOBAL".to_string(),
            "/host/npm/global".to_string(),
        ));
    }

    if info.npm_local_dir.is_some() {
        env_vars.push(("HOST_NPM_LOCAL".to_string(), "/host/npm/local".to_string()));
    }

    // Cargo package environment variables
    if info.cargo_registry.is_some() {
        env_vars.push((
            "HOST_CARGO_REGISTRY".to_string(),
            "/host/cargo/registry".to_string(),
        ));
    }

    if info.cargo_bin.is_some() {
        env_vars.push(("HOST_CARGO_BIN".to_string(), "/host/cargo/bin".to_string()));
    }

    env_vars
}
