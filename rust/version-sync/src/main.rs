use clap::{Parser, Subcommand};
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
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
            if current.join("package.json").exists() && current.join("rust/Cargo.toml").exists() {
                return Ok(current);
            }
            if !current.pop() {
                eprintln!("Could not find project root (no package.json found)");
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
        vec![self.project_root.join("rust/Cargo.toml")]
    }

    fn check_file_version(&self, path: &Path) -> Result<FileVersionStatus> {
        if !path.exists() {
            return Ok(FileVersionStatus::Missing);
        }

        let content = fs::read_to_string(path).map_err(|e| {
            vm_core::error::VmError::Internal(format!("Failed to read {}: {}", path.display(), e))
        })?;

        match Self::workspace_package_version(&content) {
            Some(current_version) if current_version == self.package_version => {
                Ok(FileVersionStatus::Synced)
            }
            Some(current_version) => Ok(FileVersionStatus::OutOfSync(current_version)),
            None => Ok(FileVersionStatus::NoVersion),
        }
    }

    fn update_file_version(&self, path: &Path) -> Result<bool> {
        let content = fs::read_to_string(path).map_err(|e| {
            vm_core::error::VmError::Internal(format!("Failed to read {}: {}", path.display(), e))
        })?;

        let updated = Self::replace_workspace_package_version(&content, &self.package_version);

        if updated != content {
            fs::write(path, updated).map_err(|e| {
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

    fn workspace_package_version(content: &str) -> Option<String> {
        let mut in_workspace_package = false;

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                in_workspace_package = trimmed == "[workspace.package]";
                continue;
            }

            if in_workspace_package && trimmed.starts_with("version") {
                let (_, value) = trimmed.split_once('=')?;
                return Some(value.trim().trim_matches('"').to_string());
            }
        }

        None
    }

    fn replace_workspace_package_version(content: &str, version: &str) -> String {
        let mut in_workspace_package = false;
        let mut replaced = false;
        let mut lines = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                in_workspace_package = trimmed == "[workspace.package]";
            }

            if in_workspace_package && !replaced && trimmed.starts_with("version") {
                let indent_len = line.len() - line.trim_start().len();
                let indent = &line[..indent_len];
                lines.push(format!("{indent}version = \"{version}\""));
                replaced = true;
            } else {
                lines.push(line.to_string());
            }
        }

        let mut updated = lines.join("\n");
        if content.ends_with('\n') {
            updated.push('\n');
        }
        updated
    }

    fn check(&self) -> Result<bool> {
        println!("📦 Package version: {}", self.package_version);
        println!();
        println!("🔍 Checking version synchronization...");
        println!();

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
            println!();
            println!("✅ All versions are in sync!");
        } else {
            println!();
            println!("❌ Some versions are out of sync. Run 'sync' to fix.");
        }

        Ok(all_synced)
    }

    fn sync(&self) -> Result<()> {
        println!("📦 Package version: {}", self.package_version);
        println!();
        println!("🔄 Synchronizing versions...");
        println!();

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
            println!();
            println!("✅ All versions were already in sync!");
        } else {
            println!();
            println!(
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
            eprintln!("Version sync initialization failed: {e}");
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
                eprintln!("Version check failed: {e}");
                process::exit(1);
            }
        },
        Command::Sync => {
            if let Err(e) = version_sync.sync() {
                eprintln!("Version sync failed: {e}");
                process::exit(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::VersionSync;

    const CARGO_TOML: &str = r#"[workspace]
resolver = "2"

[workspace.package]
version = "4.8.3"
edition = "2021"

[workspace.dependencies]
serde = { version = "1.0.228", features = ["derive"] }
clap = { version = "4.6.1", features = ["derive"] }
"#;

    #[test]
    fn reads_workspace_package_version_only() {
        assert_eq!(
            VersionSync::workspace_package_version(CARGO_TOML).as_deref(),
            Some("4.8.3")
        );
    }

    #[test]
    fn updates_workspace_package_version_only() {
        let updated = VersionSync::replace_workspace_package_version(CARGO_TOML, "4.8.4");

        assert!(updated.contains("[workspace.package]\nversion = \"4.8.4\""));
        assert!(updated.contains("serde = { version = \"1.0.228\""));
        assert!(updated.contains("clap = { version = \"4.6.1\""));
        assert!(!updated.contains("version = \"4.8.3\""));
    }
}
