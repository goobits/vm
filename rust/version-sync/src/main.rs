use clap::{Parser, Subcommand};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::process;
use tracing::{error, info};
use vm_core::error::Result;

#[derive(Parser)]
#[command(name = "version-sync")]
#[command(about = "Synchronize version references across VM project files")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check if all versions are synchronized
    Check,
    /// Update all version references to match package.json
    Sync,
}

struct VersionSync {
    project_root: PathBuf,
    package_version: String,
}

impl VersionSync {
    fn new() -> Result<Self> {
        let project_root = Self::find_project_root()?;
        let package_version = Self::read_package_version(&project_root)?;

        Ok(Self {
            project_root,
            package_version,
        })
    }

    fn find_project_root() -> Result<PathBuf> {
        let mut current = std::env::current_dir()?;
        loop {
            if current.join("package.json").exists() {
                return Ok(current);
            }
            if !current.pop() {
                error!("Could not find project root (no package.json found)");
                return Err(vm_core::error::VmError::Internal(
                    "Could not find project root".to_string(),
                ));
            }
        }
    }

    fn read_package_version(root: &Path) -> Result<String> {
        let package_json = fs::read_to_string(root.join("package.json")).map_err(|e| {
            vm_core::error::VmError::Internal(format!("Failed to read package.json: {e}"))
        })?;

        let json: serde_json::Value = serde_json::from_str(&package_json).map_err(|e| {
            vm_core::error::VmError::Internal(format!("Failed to parse package.json: {e}"))
        })?;

        json.get("version")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                vm_core::error::VmError::Internal(
                    "No version field found in package.json".to_string(),
                )
            })
    }

    fn files_to_sync(&self) -> Vec<PathBuf> {
        vec![
            self.project_root.join("rust/Cargo.toml"),
            self.project_root.join("configs/defaults.yaml"),
            self.project_root
                .join("rust/version-sync/fixtures/config.yaml"),
            self.project_root.join("rust/version-sync/fixtures/vm.yaml"),
        ]
    }

    fn check_file_version(&self, path: &Path) -> Result<FileVersionStatus> {
        if !path.exists() {
            return Ok(FileVersionStatus::Missing);
        }

        let content = fs::read_to_string(path).map_err(|e| {
            vm_core::error::VmError::Internal(format!("Failed to read {}: {}", path.display(), e))
        })?;

        static VERSION_REGEX: OnceLock<Regex> = OnceLock::new();
        let version_regex = VERSION_REGEX.get_or_init(|| {
            Regex::new(r#"version\s*[:=]\s*"?([^"\s]+)"?"#).expect("Invalid regex")
        });

        if let Some(captures) = version_regex.captures(&content) {
            let current_version = captures.get(1).map(|m| m.as_str()).unwrap_or("unknown");
            if current_version == self.package_version {
                Ok(FileVersionStatus::Synced)
            } else {
                Ok(FileVersionStatus::OutOfSync(current_version.to_string()))
            }
        } else {
            Ok(FileVersionStatus::NoVersion)
        }
    }

    fn update_file_version(&self, path: &Path) -> Result<bool> {
        let content = fs::read_to_string(path).map_err(|e| {
            vm_core::error::VmError::Internal(format!("Failed to read {}: {}", path.display(), e))
        })?;

        static VERSION_REGEX: OnceLock<Regex> = OnceLock::new();
        let version_regex = VERSION_REGEX.get_or_init(|| {
            Regex::new(r#"version\s*=\s*"[^"]+""#).expect("Invalid regex")
        });
        static YAML_VERSION_REGEX: OnceLock<Regex> = OnceLock::new();
        let yaml_version_regex = YAML_VERSION_REGEX.get_or_init(|| {
            Regex::new(r#"version:\s*"?[^"\s]+"?"#).expect("Invalid regex")
        });

        let updated = if version_regex.is_match(&content) {
            version_regex.replace_all(
                &content,
                &format!(r#"version = "{}""#, self.package_version),
            )
        } else {
            yaml_version_regex
                .replace_all(&content, &format!(r#"version: "{}""#, self.package_version))
        };

        if updated != content {
            fs::write(path, updated.as_ref()).map_err(|e| {
                vm_core::error::VmError::Internal(format!(
                    "Failed to write {}: {}",
                    path.display(),
                    e
                ))
            })?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn check(&self) -> Result<bool> {
        info!("📦 Package version: {}", self.package_version);
        info!("");
        info!("🔍 Checking version synchronization...");
        info!("");

        let mut all_synced = true;

        for file_path in self.files_to_sync() {
            let relative_path = file_path
                .strip_prefix(&self.project_root)
                .unwrap_or(&file_path);

            match self.check_file_version(&file_path)? {
                FileVersionStatus::Synced => {
                    info!("✅ {} ({})", relative_path.display(), self.package_version);
                }
                FileVersionStatus::OutOfSync(current) => {
                    info!(
                        "❌ {} ({} → should be {})",
                        relative_path.display(),
                        current,
                        self.package_version
                    );
                    all_synced = false;
                }
                FileVersionStatus::Missing => {
                    info!("⚠️  {} (missing)", relative_path.display());
                }
                FileVersionStatus::NoVersion => {
                    info!("⚠️  {} (no version found)", relative_path.display());
                }
            }
        }

        if all_synced {
            info!("");
            info!("✅ All versions are in sync!");
        } else {
            info!("");
            info!("❌ Some versions are out of sync. Run 'sync' to fix.");
        }

        Ok(all_synced)
    }

    fn sync(&self) -> Result<()> {
        info!("📦 Package version: {}", self.package_version);
        info!("");
        info!("🔄 Synchronizing versions...");
        info!("");

        let mut updated_count = 0;

        for file_path in self.files_to_sync() {
            let relative_path = file_path
                .strip_prefix(&self.project_root)
                .unwrap_or(&file_path);

            match self.check_file_version(&file_path)? {
                FileVersionStatus::OutOfSync(current) => {
                    if self.update_file_version(&file_path)? {
                        info!(
                            "✅ Updated {}: {} → {}",
                            relative_path.display(),
                            current,
                            self.package_version
                        );
                        updated_count += 1;
                    }
                }
                FileVersionStatus::Synced => {
                    info!(
                        "✅ {} already up to date ({})",
                        relative_path.display(),
                        self.package_version
                    );
                }
                FileVersionStatus::Missing => {
                    info!("⚠️  {} (missing)", relative_path.display());
                }
                FileVersionStatus::NoVersion => {
                    info!("⚠️  {} (no version found)", relative_path.display());
                }
            }
        }

        if updated_count == 0 {
            info!("");
            info!("✅ All versions were already in sync!");
        } else {
            info!("");
            info!(
                "✅ Updated {} files to version {}",
                updated_count, self.package_version
            );
        }

        Ok(())
    }
}

#[derive(Debug)]
enum FileVersionStatus {
    Synced,
    OutOfSync(String),
    Missing,
    NoVersion,
}

fn main() {
    let cli = Cli::parse();

    let version_sync = match VersionSync::new() {
        Ok(vs) => vs,
        Err(e) => {
            error!("Version sync initialization failed: {}", e);
            process::exit(1);
        }
    };

    match cli.command {
        Command::Check => match version_sync.check() {
            Ok(all_synced) => {
                if all_synced {
                    process::exit(0);
                } else {
                    process::exit(1);
                }
            }
            Err(e) => {
                error!("Version check failed: {}", e);
                process::exit(1);
            }
        },
        Command::Sync => {
            if let Err(e) = version_sync.sync() {
                error!("Version sync failed: {}", e);
                process::exit(1);
            }
        }
    }
}
