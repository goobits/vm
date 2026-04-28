use super::TartProvisioner;
use crate::{THEMES_JSON, ZSHRC_TEMPLATE};
use serde_json::json;
use tera::{Context, Tera};
use tracing::warn;
use vm_config::config::VmConfig;
use vm_core::error::{Result, VmError};

impl TartProvisioner {
    fn is_valid_shell_identifier(key: &str) -> bool {
        let mut chars = key.chars();
        match chars.next() {
            Some(c) if c == '_' || c.is_ascii_alphabetic() => {}
            _ => return false,
        }

        chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
    }

    pub(crate) fn apply_shell_overrides(&self, config: &VmConfig) -> Result<()> {
        let Some(overrides) = Self::render_shell_overrides(config) else {
            return Ok(());
        };

        let script = format!(
            r#"cat > "$HOME/.vm_shell_overrides" <<'EOF'
{overrides}
EOF
touch "$HOME/.bashrc"
if ! grep -Fq '. "$HOME/.vm_shell_overrides"' "$HOME/.bashrc"; then
  printf '\n[ -f "$HOME/.vm_shell_overrides" ] && . "$HOME/.vm_shell_overrides"\n' >> "$HOME/.bashrc"
fi"#
        );

        self.ssh_exec(&script)?;
        Ok(())
    }

    pub(crate) fn apply_canonical_shell_config(&self, config: &VmConfig) -> Result<()> {
        let rendered = Self::render_canonical_zshrc(config, &self.project_dir)?;

        self.ssh_exec(&format!(
            r#"cat > "$HOME/.zshrc" <<'EOF'
{}
EOF
touch "$HOME/.bashrc"
if ! grep -Fq '. "$HOME/.vm_shell_overrides"' "$HOME/.bashrc"; then
  printf '\n[ -f "$HOME/.vm_shell_overrides" ] && . "$HOME/.vm_shell_overrides"\n' >> "$HOME/.bashrc"
fi
if command -v chsh >/dev/null 2>&1 && command -v zsh >/dev/null 2>&1; then
  chsh -s "$(command -v zsh)" "$USER" >/dev/null 2>&1 || true
fi"#,
            rendered
        ))?;

        Ok(())
    }

    pub(crate) fn render_shell_overrides(config: &VmConfig) -> Option<String> {
        let mut lines = Vec::new();

        for (key, value) in &config.environment {
            if !Self::is_valid_shell_identifier(key) {
                warn!("Skipping invalid shell environment key for Tart provisioning: {key}");
                continue;
            }

            lines.push(format!(
                "export {}='{}'",
                key,
                Self::shell_escape_single_quotes(value)
            ));
        }

        if lines.is_empty() {
            None
        } else {
            Some(lines.join("\n"))
        }
    }

    pub(crate) fn render_canonical_zshrc(config: &VmConfig, project_path: &str) -> Result<String> {
        let mut tera = Tera::default();
        tera.add_raw_template("zshrc", ZSHRC_TEMPLATE)
            .map_err(|e| VmError::Internal(format!("Failed to load zshrc template: {e}")))?;

        let themes: serde_json::Value = serde_json::from_str(THEMES_JSON)
            .map_err(|e| VmError::Internal(format!("Failed to parse themes.json: {e}")))?;

        let terminal = config.terminal.clone().unwrap_or_default();
        let theme_name = terminal.theme.unwrap_or_else(|| "dracula".to_string());
        let colors = themes
            .get(&theme_name)
            .and_then(|t| t.get("colors"))
            .cloned()
            .or_else(|| themes.get("dracula").and_then(|t| t.get("colors")).cloned())
            .unwrap_or_else(|| {
                json!({
                    "foreground": "#f8f8f2",
                    "background": "#282a36",
                    "red": "#ff5555",
                    "green": "#50fa7b",
                    "yellow": "#f1fa8c",
                    "blue": "#bd93f9",
                    "magenta": "#ff79c6",
                    "cyan": "#8be9fd",
                    "bright_black": "#6272a4"
                })
            });

        let project_name = config
            .project
            .as_ref()
            .and_then(|p| p.name.clone())
            .unwrap_or_else(|| Self::default_project_name(project_path));
        let project_path_b64 = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(project_path)
        };
        let project_aliases = config
            .aliases
            .iter()
            .filter(|(key, _)| {
                let mut chars = key.chars();
                matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
                    && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            })
            .map(|(key, value)| {
                use base64::Engine;
                json!({
                    "key": key,
                    "value_b64": base64::engine::general_purpose::STANDARD.encode(value)
                })
            })
            .collect::<Vec<_>>();
        let project_ports = config
            .ports
            .mappings
            .iter()
            .map(|mapping| {
                json!({
                    "key": format!("{}/{}", mapping.host, format!("{:?}", mapping.protocol).to_lowercase()),
                    "value": mapping.guest
                })
            })
            .collect::<Vec<_>>();

        let mut context = Context::new();
        context.insert("project_name", &project_name);
        context.insert("project_path", project_path);
        context.insert("project_path_b64", &project_path_b64);
        context.insert(
            "project_config",
            &serde_json::to_value(config).unwrap_or_else(|_| json!({})),
        );
        context.insert(
            "terminal_emoji",
            &terminal
                .emoji
                .unwrap_or_else(|| Self::default_terminal_emoji(config).to_string()),
        );
        context.insert(
            "terminal_username",
            &terminal.username.unwrap_or_else(|| "dev".to_string()),
        );
        context.insert("show_git_branch", &terminal.show_git_branch.unwrap_or(true));
        context.insert("show_timestamp", &terminal.show_timestamp.unwrap_or(false));
        context.insert("terminal_colors", &colors);
        context.insert("project_aliases", &project_aliases);
        context.insert("project_ports", &project_ports);

        tera.render("zshrc", &context)
            .map_err(|e| VmError::Internal(format!("Failed to render zshrc template: {e}")))
    }

    fn default_terminal_emoji(config: &VmConfig) -> &'static str {
        match Self::guest_os(config) {
            "macos" => "🍎",
            "linux" => "🐧",
            _ => "🚀",
        }
    }

    fn default_project_name(project_path: &str) -> String {
        std::path::Path::new(project_path)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("project")
            .to_string()
    }
}
