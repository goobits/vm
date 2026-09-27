//! Authenticated HTTP client operations for the auth proxy.

use crate::storage::{get_auth_data_dir, read_auth_token};
use crate::types::{EnvironmentResponse, SecretListResponse, SecretRequest, SecretScope};
use anyhow::{anyhow, Context, Result};
use reqwest::{Client, Response};
use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;

fn parse_secret_scope(scope: Option<&str>) -> Result<SecretScope> {
    let value = scope.unwrap_or("global");
    SecretScope::parse(value).ok_or_else(|| {
        anyhow!("Invalid scope '{value}'. Use 'global', 'project:NAME', or 'instance:NAME'")
    })
}

fn endpoint_url(server_url: &str, segments: &[&str]) -> Result<reqwest::Url> {
    let mut url = reqwest::Url::parse(server_url).context("Invalid auth proxy URL")?;
    let loopback = url
        .host_str()
        .and_then(|host| {
            host.trim_start_matches('[')
                .trim_end_matches(']')
                .parse::<IpAddr>()
                .ok()
        })
        .is_some_and(|address| address.is_loopback());
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err(anyhow!(
            "Auth proxy requires HTTPS or a literal loopback HTTP address"
        ));
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(anyhow!(
            "Auth proxy base URL must not contain credentials, query parameters, or a fragment"
        ));
    }
    url.path_segments_mut()
        .map_err(|_| anyhow!("Auth proxy URL cannot be used as a base"))?
        .pop_if_empty()
        .extend(segments);
    Ok(url)
}

fn scoped_url(server_url: &str, segments: &[&str], scope: &str) -> Result<reqwest::Url> {
    parse_secret_scope(Some(scope))?;
    let mut url = endpoint_url(server_url, segments)?;
    url.query_pairs_mut().append_pair("scope", scope);
    Ok(url)
}

// Secret traffic must not be forwarded through environment-selected proxies or
// redirects, including a local endpoint redirecting to a remote HTTP origin.
fn auth_client() -> Result<Client> {
    Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
        .context("Failed to initialize auth proxy client")
}

async fn auth_token() -> Result<String> {
    read_auth_token(&get_auth_data_dir()?).context("Failed to read local auth token")
}

async fn response_or_error(response: Response, operation: &str) -> Result<Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    // Error bodies may echo submitted credentials; only expose the status.
    Err(anyhow!("Failed to {operation}: {status}"))
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
    let response = auth_client()?
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
pub async fn list_secrets(server_url: &str, scope: &str) -> Result<SecretListResponse> {
    let response = auth_client()?
        .get(scoped_url(server_url, &["secrets"], scope)?)
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
pub async fn remove_secret(server_url: &str, name: &str, scope: &str) -> Result<()> {
    let response = auth_client()?
        .delete(scoped_url(server_url, &["secrets", name], scope)?)
        .bearer_auth(auth_token().await?)
        .send()
        .await
        .context("Failed to send request to auth proxy")?;
    response_or_error(response, "remove secret").await?;
    Ok(())
}

/// Return the plaintext value for one secret.
pub async fn get_secret_value(server_url: &str, name: &str, scope: &str) -> Result<String> {
    let response = auth_client()?
        .get(scoped_url(server_url, &["secrets", name], scope)?)
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
    let client = auth_client()?;
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
    use super::{auth_client, endpoint_url, parse_secret_scope, response_or_error};
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
    #[test]
    fn rejects_remote_cleartext_and_ambiguous_base_urls() {
        for base in [
            "http://example.com",
            "http://127.0.0.1.example.com",
            "http://localhost",
            "http://0.0.0.0",
            "ftp://127.0.0.1",
            "https://user:password@example.com",
            "https://example.com?token=secret",
            "https://example.com#fragment",
        ] {
            assert!(endpoint_url(base, &["secrets"]).is_err(), "{base}");
        }
        for base in [
            "https://example.com",
            "http://127.0.0.1",
            "http://[::1]:3090",
        ] {
            assert!(endpoint_url(base, &["secrets"]).is_ok(), "{base}");
        }
    }

    #[tokio::test]
    async fn credential_requests_do_not_follow_redirects_or_expose_error_bodies() {
        use axum::{http::StatusCode, response::Redirect, routing::get, Router};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new()
            .route(
                "/redirect",
                get(|| async { Redirect::temporary("/target") }),
            )
            .route("/target", get(|| async { "redirect was followed" }))
            .route(
                "/error",
                get(|| async { (StatusCode::BAD_REQUEST, "echoed-secret-sentinel") }),
            );
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = auth_client().unwrap();
        let redirect = client
            .get(format!("http://{address}/redirect"))
            .bearer_auth("test-only-transport-token")
            .send()
            .await
            .unwrap();
        assert_eq!(redirect.status(), StatusCode::TEMPORARY_REDIRECT);
        assert!(response_or_error(redirect, "read secret").await.is_err());
        let response = client
            .get(format!("http://{address}/error"))
            .send()
            .await
            .unwrap();
        let error = response_or_error(response, "read secret")
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("400"));
        assert!(!error.contains("echoed-secret-sentinel"));
        server.abort();
        let _ = server.await;
    }
}
