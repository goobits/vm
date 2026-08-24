//! Authenticated HTTP client operations for the auth proxy.

use crate::storage::{get_auth_data_dir, SecretStore};
use crate::types::{EnvironmentResponse, SecretListResponse, SecretRequest, SecretScope};
use anyhow::{anyhow, Context, Result};
use reqwest::{Client, Response};
use std::collections::HashMap;

fn parse_secret_scope(scope: Option<&str>) -> Result<SecretScope> {
    match scope {
        Some("global") | None => Ok(SecretScope::Global),
        Some(value) => {
            if let Some(name) = value
                .strip_prefix("project:")
                .filter(|name| !name.is_empty())
            {
                return Ok(SecretScope::Project(name.to_string()));
            }
            if let Some(name) = value
                .strip_prefix("instance:")
                .filter(|name| !name.is_empty())
            {
                return Ok(SecretScope::Instance(name.to_string()));
            }
            Err(anyhow!(
                "Invalid scope '{value}'. Use 'global', 'project:NAME', or 'instance:NAME'"
            ))
        }
    }
}

fn endpoint_url(server_url: &str, segments: &[&str]) -> Result<reqwest::Url> {
    let mut url = reqwest::Url::parse(server_url).context("Invalid auth proxy URL")?;
    url.path_segments_mut()
        .map_err(|_| anyhow!("Auth proxy URL cannot be used as a base"))?
        .pop_if_empty()
        .extend(segments);
    Ok(url)
}

async fn auth_token() -> Result<String> {
    let store =
        SecretStore::new(get_auth_data_dir()?).context("Failed to open local secret store")?;
    store
        .get_auth_token()
        .map(str::to_string)
        .ok_or_else(|| anyhow!("No authentication token found. Is the auth service running?"))
}

async fn response_or_error(response: Response, operation: &str) -> Result<Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let message = response.text().await.unwrap_or_default();
    Err(anyhow!("Failed to {operation}: {status} - {message}"))
}

/// Add or replace one secret.
pub async fn add_secret(
    server_url: &str,
    name: &str,
    value: &str,
    scope: Option<&str>,
    description: Option<&str>,
) -> Result<()> {
    let request = SecretRequest {
        value: value.to_string(),
        scope: parse_secret_scope(scope)?,
        description: description.map(str::to_string),
    };
    let response = Client::new()
        .post(endpoint_url(server_url, &["secrets", name])?)
        .bearer_auth(auth_token().await?)
        .json(&request)
        .send()
        .await
        .context("Failed to send request to auth proxy")?;
    response_or_error(response, "add secret").await?;
    Ok(())
}

/// Return secret metadata without values.
pub async fn list_secrets(server_url: &str) -> Result<SecretListResponse> {
    let response = Client::new()
        .get(endpoint_url(server_url, &["secrets"])?)
        .bearer_auth(auth_token().await?)
        .send()
        .await
        .context("Failed to send request to auth proxy")?;
    response_or_error(response, "list secrets")
        .await?
        .json()
        .await
        .context("Failed to parse auth proxy response")
}

/// Remove one secret.
pub async fn remove_secret(server_url: &str, name: &str) -> Result<()> {
    let response = Client::new()
        .delete(endpoint_url(server_url, &["secrets", name])?)
        .bearer_auth(auth_token().await?)
        .send()
        .await
        .context("Failed to send request to auth proxy")?;
    response_or_error(response, "remove secret").await?;
    Ok(())
}

/// Return the plaintext value for one secret.
pub async fn get_secret_value(server_url: &str, name: &str) -> Result<String> {
    let response = Client::new()
        .get(endpoint_url(server_url, &["secrets", name])?)
        .bearer_auth(auth_token().await?)
        .send()
        .await
        .context("Failed to send request to auth proxy")?;
    response_or_error(response, "get secret")
        .await?
        .text()
        .await
        .context("Failed to read auth proxy response")
}

/// Return the environment variables visible to one VM.
pub async fn get_secret_for_vm(
    server_url: &str,
    vm_name: &str,
    project_name: Option<&str>,
) -> Result<HashMap<String, String>> {
    let client = Client::new();
    let mut url = endpoint_url(server_url, &["env", vm_name])?;
    if let Some(project) = project_name {
        url.query_pairs_mut().append_pair("project", project);
    }
    let response = client
        .get(url)
        .bearer_auth(auth_token().await?)
        .send()
        .await
        .context("Failed to send request to auth proxy")?;
    let environment: EnvironmentResponse = response_or_error(response, "get VM environment")
        .await?
        .json()
        .await
        .context("Failed to parse auth proxy response")?;
    Ok(environment.env_vars)
}

#[cfg(test)]
mod tests {
    use super::{endpoint_url, parse_secret_scope};
    use crate::types::SecretScope;

    #[test]
    fn parses_only_supported_secret_scopes() {
        assert_eq!(parse_secret_scope(None).unwrap(), SecretScope::Global);
        assert_eq!(
            parse_secret_scope(Some("project:demo")).unwrap(),
            SecretScope::Project("demo".into())
        );
        assert!(parse_secret_scope(Some("team:demo")).is_err());
        assert!(parse_secret_scope(Some("project:")).is_err());
    }

    #[test]
    fn endpoint_segments_are_encoded() {
        assert_eq!(
            endpoint_url("http://127.0.0.1:3090", &["secrets", "team/token"])
                .unwrap()
                .as_str(),
            "http://127.0.0.1:3090/secrets/team%2Ftoken"
        );
    }
}
