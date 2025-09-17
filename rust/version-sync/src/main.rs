use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use vm_common::vm_error;

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
                vm_error!("Could not find project root (no package.json found)");
                return Err(anyhow::anyhow!("Could not find project root"));
            }
        }
    }

    fn read_package_version(root: &Path) -> Result<String> {
        let package_json =
            fs::read_to_string(root.join("package.json")).context("Failed to read package.json")?;

        let json: serde_json::Value =
            serde_json::from_str(&package_json).context("Failed to parse package.json")?;

        json.get("version")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .context("No version field found in package.json")
    }

    fn files_to_sync(&self) -> Vec<PathBuf> {
        vec![
            self.project_root.join("rust/Cargo.toml"),
            self.project_root.join("defaults.yaml"),
            self.project_root.join("rust/version-sync/fixtures/config.yaml"),
            self.project_root.join("rust/version-sync/fixtures/vm.yaml"),
        ]
    }

    fn check_file_version(&self, path: &Path) -> Result<FileVersionStatus> {
        if !path.exists() {
            return Ok(FileVersionStatus::Missing);
        }

        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        let version_regex = Regex::new(r#"version\s*[:=]\s*"?([^"\s]+)"?"#)
            .expect("Invalid regex pattern for version detection");

        if let Some(captures) = version_regex.captures(&content) {
            let current_version = captures.get(1)
                .expect("Regex should have captured version group")
                .as_str();
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
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        let version_regex = Regex::new(r#"version\s*=\s*"[^"]+""#)
            .expect("Invalid regex pattern for version replacement");
        let yaml_version_regex = Regex::new(r#"version:\s*"?[^"\s]+"?"#)
            .expect("Invalid regex pattern for YAML version replacement");

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
            fs::write(path, updated.as_ref())
                .with_context(|| format!("Failed to write {}", path.display()))?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn check(&self) -> Result<bool> {
        println!("📦 Package version: {}", self.package_version);
        println!("\n🔍 Checking version synchronization...\n");

        let mut all_synced = true;

        for file_path in self.files_to_sync() {
            let relative_path = file_path
                .strip_prefix(&self.project_root)
                .unwrap_or(&file_path);

            match self.check_file_version(&file_path)? {
                FileVersionStatus::Synced => {
                    println!("✅ {} ({})", relative_path.display(), self.package_version);
                }
                FileVersionStatus::OutOfSync(current) => {
                    println!(
                        "❌ {} ({} → should be {})",
                        relative_path.display(),
                        current,
                        self.package_version
                    );
                    all_synced = false;
                }
                FileVersionStatus::Missing => {
                    println!("⚠️  {} (missing)", relative_path.display());
                }
                FileVersionStatus::NoVersion => {
                    println!("⚠️  {} (no version found)", relative_path.display());
                }
            }
        }

        if all_synced {
            println!("\n✅ All versions are in sync!");
        } else {
            println!("\n❌ Some versions are out of sync. Run 'sync' to fix.");
        }

        Ok(all_synced)
    }

    fn sync(&self) -> Result<()> {
        println!("📦 Package version: {}", self.package_version);
        println!("\n🔄 Synchronizing versions...\n");

        let mut updated_count = 0;

        for file_path in self.files_to_sync() {
            let relative_path = file_path
                .strip_prefix(&self.project_root)
                .unwrap_or(&file_path);

            match self.check_file_version(&file_path)? {
                FileVersionStatus::OutOfSync(current) => {
                    if self.update_file_version(&file_path)? {
                        println!(
                            "✅ Updated {}: {} → {}",
                            relative_path.display(),
                            current,
                            self.package_version
                        );
                        updated_count += 1;
                    }
                }
                FileVersionStatus::Synced => {
                    println!(
                        "✅ {} already up to date ({})",
                        relative_path.display(),
                        self.package_version
                    );
                }
                FileVersionStatus::Missing => {
                    println!("⚠️  {} (missing)", relative_path.display());
                }
                FileVersionStatus::NoVersion => {
                    println!("⚠️  {} (no version found)", relative_path.display());
                }
            }
        }

        if updated_count == 0 {
            println!("\n✅ All versions were already in sync!");
        } else {
            println!(
                "\n✅ Updated {} files to version {}",
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
            eprintln!("Error: {}", e);
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
                eprintln!("Error: {}", e);
                process::exit(1);
            }
        },
        Command::Sync => {
            if let Err(e) = version_sync.sync() {
                eprintln!("Error: {}", e);
                process::exit(1);
            }
        }
    }
}
