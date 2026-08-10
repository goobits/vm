use super::TartProvisioner;
use tracing::info;
use vm_config::config::VmConfig;

impl TartProvisioner {
    pub(super) fn database_command(&self, config: &VmConfig) -> Option<String> {
        let postgresql = config
            .services
            .get("postgresql")
            .or_else(|| config.services.get("postgres"))
            .map(|s| s.enabled)
            .unwrap_or(false);
        let redis = config
            .services
            .get("redis")
            .map(|s| s.enabled)
            .unwrap_or(false);
        let mongodb = config
            .services
            .get("mongodb")
            .map(|s| s.enabled)
            .unwrap_or(false);
        if !postgresql && !redis && !mongodb {
            return None;
        }

        if postgresql {
            info!("Installing PostgreSQL");
        }
        if redis {
            info!("Installing Redis");
        }
        if mongodb {
            info!("Installing MongoDB");
        }

        let mut commands = Vec::new();
        if self.is_macos_guest(config) {
            commands.push(Self::homebrew_preamble().to_string());
            if postgresql {
                commands.push("brew install postgresql".to_string());
                commands.push("brew services start postgresql || true".to_string());
            }
            if redis {
                commands.push("brew install redis".to_string());
                commands.push("brew services start redis || true".to_string());
            }
            if mongodb {
                commands.push("brew tap mongodb/brew || true".to_string());
                commands.push("brew install mongodb-community || true".to_string());
                commands.push("brew services start mongodb-community || true".to_string());
            }
        } else {
            let mut packages = Vec::new();
            if postgresql {
                packages.extend(["postgresql", "postgresql-contrib"]);
            }
            if redis {
                packages.push("redis-server");
            }
            if mongodb {
                packages.push("mongodb");
            }
            commands.push(format!(
                "sudo apt-get update && sudo apt-get install -y {}",
                packages.join(" ")
            ));
            if postgresql {
                commands.push("sudo systemctl enable --now postgresql".to_string());
            }
            if redis {
                commands.push("sudo systemctl enable --now redis-server".to_string());
            }
            if mongodb {
                commands.push("sudo systemctl enable --now mongodb".to_string());
            }
        }

        Some(commands.join("\n"))
    }

    pub(super) fn custom_provision_command(&self) -> String {
        let project = Self::shell_escape_single_quotes(&self.project_dir);
        format!("if [ -f '{project}/provision.sh' ]; then cd '{project}' && bash provision.sh; fi")
    }
}
