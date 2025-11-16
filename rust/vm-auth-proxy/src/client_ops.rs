//! Client operations for auth proxy CLI commands

use crate::storage::{get_auth_data_dir, SecretStore};
use crate::types::{SecretListResponse, SecretRequest, SecretScope, SecretSummary};
use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use reqwest::Client;
use std::collections::HashMap;
use tracing::{debug, info, warn};
use vm_cli::msg;
use vm_messages::messages::MESSAGES;

/// Parse a scope string into a SecretScope enum
fn parse_secret_scope(scope: Option<&str>) -> Result<SecretScope> {
    match scope {
        Some("global") | None => Ok(SecretScope::Global),
        Some(s) if s.starts_with("project:") => {
            let project_name = s
                .strip_prefix("project:")
                .ok_or_else(|| anyhow!("Invalid project scope format, expected 'project:NAME'"))?;
            Ok(SecretScope::Project(project_name.to_string()))
        }
        Some(s) if s.starts_with("instance:") => {
            let instance_name = s.strip_prefix("instance:").ok_or_else(|| {
                anyhow!("Invalid instance scope format, expected 'instance:NAME'")
            })?;
            Ok(SecretScope::Instance(instance_name.to_string()))
        }
        Some(s) => Err(anyhow!(
            "Invalid scope '{s}'. Use 'global', 'project:NAME', or 'instance:NAME'"
        )),
    }
}

/// Add a secret to the auth proxy
pub async fn add_secret(
    server_url: &str,
    name: &str,
    value: &str,
    scope: Option<&str>,
    description: Option<&str>,
) -> Result<()> {
    // Parse scope
    let secret_scope = parse_secret_scope(scope)?;

    let request = SecretRequest {
        value: value.to_string(),
        scope: secret_scope,
        description: description.map(|s| s.to_string()),
    };

    let auth_token = get_auth_token().await?;
    let client = Client::new();
    let url = format!("{server_url}/secrets/{name}");

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {auth_token}"))
        .json(&request)
        .send()
        .await
        .context("Failed to send request to auth proxy")?;

    if response.status().is_success() {
        info!("{}", msg!(MESSAGES.service.auth_secret_added, name = name));
    } else {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(anyhow!("Failed to add secret: {status} - {error_text}"));
    }

    Ok(())
}

/// List all secrets
pub async fn list_secrets(server_url: &str, show_values: bool) -> Result<()> {
    let auth_token = get_auth_token().await?;
    let client = Client::new();
    let url = format!("{server_url}/secrets");

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {auth_token}"))
        .send()
        .await
        .context("Failed to send request to auth proxy")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(anyhow!("Failed to list secrets: {status} - {error_text}"));
    }

    let list: SecretListResponse = response.json().await.context("Failed to parse response")?;

    if list.secrets.is_empty() {
        info!("{}", MESSAGES.service.auth_secrets_empty);
        return Ok(());
    }

    info!(
        "{}",
        msg!(
            MESSAGES.service.auth_secrets_list_header,
            count = list.total.to_string()
        )
    );

    for secret in &list.secrets {
        print_secret_summary(secret, show_values, server_url, &auth_token).await?;
    }

    if !show_values {
        info!("{}", MESSAGES.service.auth_secrets_show_values_hint);
    }

    Ok(())
}

/// Remove a secret
pub async fn remove_secret(server_url: &str, name: &str, force: bool) -> Result<()> {
    // Confirm removal unless forced
    if !force {
        let confirm = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("Remove secret '{}'?", name.bright_yellow()))
            .default(false)
            .interact()?;

        if !confirm {
            info!("{}", MESSAGES.service.auth_remove_cancelled);
            return Ok(());
        }
    }

    let auth_token = get_auth_token().await?;
    let client = Client::new();
    let url = format!("{server_url}/secrets/{name}");

    let response = client
        .delete(&url)
        .header("Authorization", format!("Bearer {auth_token}"))
        .send()
        .await
        .context("Failed to send request to auth proxy")?;

    if response.status().is_success() {
        info!(
            "{}",
            msg!(MESSAGES.service.auth_secret_removed, name = name)
        );
    } else {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(anyhow!("Failed to remove secret: {status} - {error_text}"));
    }

    Ok(())
}

/// Get environment variables for a VM
pub async fn get_secret_for_vm(
    server_url: &str,
    vm_name: &str,
    project_name: Option<&str>,
) -> Result<HashMap<String, String>> {
    let auth_token = get_auth_token().await?;
    let client = Client::new();
    let mut url = format!("{server_url}/env/{vm_name}");

    if let Some(project) = project_name {
        url.push_str(&format!("?project={project}"));
    }

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {auth_token}"))
        .send()
        .await
        .context("Failed to send request to auth proxy")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(anyhow!(
            "Failed to get environment variables: {status} - {error_text}"
        ));
    }

    let env_response: crate::types::EnvironmentResponse =
        response.json().await.context("Failed to parse response")?;

    Ok(env_response.env_vars)
}

/// Check if the auth proxy server is running
pub async fn check_server_running(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{port}/health");
    match reqwest::get(&url).await {
        Ok(response) => response.status().is_success(),
        Err(_) => false,
    }
}

/// Interactive secret addition
pub async fn add_secret_interactive(server_url: &str) -> Result<()> {
    let theme = ColorfulTheme::default();

    // Get secret name
    let name: String = Input::with_theme(&theme)
        .with_prompt("Secret name")
        .interact_text()?;

    // Get secret value
    let value: String = Input::with_theme(&theme)
        .with_prompt("Secret value")
        .interact_text()?;

    // Get scope
    let scope_options = vec!["Global (all VMs)", "Project-specific", "Instance-specific"];
    let scope_index = Select::with_theme(&theme)
        .with_prompt("Secret scope")
        .items(&scope_options)
        .default(0)
        .interact()?;

    let scope = match scope_index {
        0 => None, // Global
        1 => {
            let project: String = Input::with_theme(&theme)
                .with_prompt("Project name")
                .interact_text()?;
            Some(format!("project:{project}"))
        }
        2 => {
            let instance: String = Input::with_theme(&theme)
                .with_prompt("Instance name")
                .interact_text()?;
            Some(format!("instance:{instance}"))
        }
        _ => None,
    };

    // Get optional description
    let description: String = Input::with_theme(&theme)
        .with_prompt("Description (optional)")
        .allow_empty(true)
        .interact_text()?;

    let description = if description.is_empty() {
        None
    } else {
        Some(description)
    };

    add_secret(
        server_url,
        &name,
        &value,
        scope.as_deref(),
        description.as_deref(),
    )
    .await
}

/// Print a secret summary
async fn print_secret_summary(
    secret: &SecretSummary,
    show_values: bool,
    server_url: &str,
    auth_token: &str,
) -> Result<()> {
    let scope_display = match &secret.scope {
        SecretScope::Global => "Global".bright_blue(),
        SecretScope::Project(p) => format!("Project: {p}").bright_yellow(),
        SecretScope::Instance(i) => format!("Instance: {i}").bright_magenta(),
    };

    let mut summary = format!(
        "  {} {} {}",
        "•".bright_green(),
        secret.name.bright_white(),
        scope_display
    );

    if show_values {
        // Fetch the actual value
        match get_secret_value(server_url, &secret.name, auth_token).await {
            Ok(value) => {
                let masked_value = if value.len() > 20 {
                    format!("{}...", &value[..17])
                } else {
                    value
                };
                summary.push_str(&format!(" = {}", masked_value.bright_cyan()));
            }
            Err(e) => {
                warn!("Failed to fetch value for {}: {}", secret.name, e);
                summary.push_str(&format!(" = {}", "<error>".bright_red()));
            }
        }
    }

    if let Some(desc) = &secret.description {
        summary.push_str(&format!(" - {}", desc.dimmed()));
    }

    info!("{}", summary);
    Ok(())
}

/// Get a specific secret value
async fn get_secret_value(server_url: &str, name: &str, auth_token: &str) -> Result<String> {
    let client = Client::new();
    let url = format!("{server_url}/secrets/{name}");

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {auth_token}"))
        .send()
        .await
        .context("Failed to send request")?;

    if response.status().is_success() {
        response.text().await.context("Failed to get response text")
    } else {
        Err(anyhow!("Failed to get secret: {}", response.status()))
    }
}

/// Get authentication token from local storage
async fn get_auth_token() -> Result<String> {
    let data_dir = get_auth_data_dir()?;
    let store = SecretStore::new(data_dir).context("Failed to open local secret store")?;

    store
        .get_auth_token()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("No authentication token found. Is the auth service running?"))
}

/// Start auth service if not running
pub async fn start_server_if_needed(port: u16) -> Result<()> {
    if check_server_running(port).await {
        debug!("Auth proxy server is already running on port {}", port);
        return Ok(());
    }

    let confirm = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Auth proxy server is not running. Start it now?")
        .default(true)
        .interact()?;

    if confirm {
        info!("{}", MESSAGES.service.auth_server_starting);

        let data_dir = get_auth_data_dir()?;

        // Start server in background
        crate::server::run_server_background("127.0.0.1".to_string(), port, data_dir).await?;

        // Verify it started
        if check_server_running(port).await {
            info!("{}", MESSAGES.service.auth_server_started);
        } else {
            return Err(anyhow!("Failed to start auth proxy server"));
        }
    } else {
        return Err(anyhow!("Auth proxy server is required but not running"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::run_server;
    use std::net::TcpListener;
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::task;
    use tracing::error;

    fn find_available_port() -> anyhow::Result<u16> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        Ok(port)
    }

    async fn start_test_server() -> (u16, tempfile::TempDir, String) {
        let temp_dir = TempDir::new().expect("should create temp dir");
        let port = find_available_port().expect("Failed to find available port");

        // Create a SecretStore to get the auth token
        let store = crate::storage::SecretStore::new(temp_dir.path().to_path_buf())
            .expect("should create secret store");
        let auth_token = store
            .get_auth_token()
            .expect("should get auth token")
            .to_string();

        let data_dir = temp_dir.path().to_path_buf();
        task::spawn(async move {
            let _ = run_server("127.0.0.1".to_string(), port, data_dir).await;
        });

        // Wait for server to start
        tokio::time::sleep(Duration::from_millis(500)).await;

        (port, temp_dir, auth_token)
    }

    /// Helper function for tests that adds a secret with provided auth token
    async fn add_secret_with_token(
        server_url: &str,
        name: &str,
        value: &str,
        scope: Option<&str>,
        description: Option<&str>,
        auth_token: &str,
    ) -> Result<()> {
        use crate::types::SecretRequest;

        // Parse scope
        let secret_scope = parse_secret_scope(scope)?;

        let request = SecretRequest {
            value: value.to_string(),
            scope: secret_scope,
            description: description.map(|s| s.to_string()),
        };

        let client = Client::new();
        let url = format!("{}/secrets/{}", server_url, name);

        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", auth_token))
            .json(&request)
            .send()
            .await
            .context("Failed to send request")?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Err(anyhow!("Failed to add secret: {} - {}", status, body))
        }
    }

    /// Helper function for tests that lists secrets with provided auth token
    async fn list_secrets_with_token(server_url: &str, auth_token: &str) -> Result<()> {
        let client = Client::new();
        let url = format!("{}/secrets", server_url);

        let response = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", auth_token))
            .send()
            .await
            .context("Failed to send request")?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Err(anyhow!("Failed to list secrets: {} - {}", status, body))
        }
    }

    #[tokio::test]
    async fn test_server_health_check() {
        let (port, _temp_dir, _auth_token) = start_test_server().await;

        // Check if server is running
        let running = check_server_running(port).await;
        assert!(running);
    }

    #[tokio::test]
    async fn test_add_and_list_secrets() {
        let (port, _temp_dir, auth_token) = start_test_server().await;
        let server_url = format!("http://127.0.0.1:{}", port);

        // Add a secret
        let result = add_secret_with_token(
            &server_url,
            "test_key",
            "test_value",
            None,
            Some("Test secret"),
            &auth_token,
        )
        .await;
        if let Err(e) = &result {
            error!("add_secret failed: {}", e);
        }
        assert!(result.is_ok());

        // List secrets
        let result = list_secrets_with_token(&server_url, &auth_token).await;
        if let Err(e) = &result {
            error!("list_secrets failed: {}", e);
        }
        assert!(result.is_ok());
    }
}
