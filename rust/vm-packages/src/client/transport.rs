use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use super::{PackageInfrastructureClient, REQUEST_TIMEOUT};

const SOURCE_SYNC_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const ERROR_BODY_LIMIT: usize = 8 * 1024;
const ERROR_DETAIL_LIMIT: usize = 512;
const REQUEST_ID_HEADER: &str = "x-request-id";

#[derive(Deserialize)]
struct ErrorEnvelope {
    error: String,
}

async fn decode_json<T: serde::de::DeserializeOwned>(
    mut response: reqwest::Response,
    service: &str,
    method: &str,
    url: &str,
) -> Result<T> {
    let status = response.status();
    if status.is_success() {
        return response
            .json()
            .await
            .with_context(|| format!("{service} returned invalid JSON from {url}"));
    }

    let request_id = response
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(|value| clean_diagnostic(value, 128));
    let mut body = Vec::new();
    while body.len() < ERROR_BODY_LIMIT {
        let Some(chunk) = response
            .chunk()
            .await
            .with_context(|| format!("failed to read {service} rejection from {url}"))?
        else {
            break;
        };
        let remaining = ERROR_BODY_LIMIT - body.len();
        body.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
    }
    let detail = rejection_detail(&body);
    let request = request_id
        .filter(|value| !value.is_empty())
        .map(|value| format!("; request ID: {value}"))
        .unwrap_or_default();
    bail!("{service} rejected {method} {url} with {status}: {detail}{request}")
}

fn rejection_detail(body: &[u8]) -> String {
    serde_json::from_slice::<ErrorEnvelope>(body)
        .ok()
        .map(|envelope| clean_diagnostic(&envelope.error, ERROR_DETAIL_LIMIT))
        .filter(|detail| !detail.is_empty())
        .unwrap_or_else(|| {
            let detail = clean_diagnostic(&String::from_utf8_lossy(body), ERROR_DETAIL_LIMIT);
            if detail.is_empty() {
                "no error detail returned".into()
            } else {
                detail
            }
        })
}

fn clean_diagnostic(value: &str, limit: usize) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(limit)
        .collect::<String>()
        .trim()
        .to_string()
}

impl PackageInfrastructureClient {
    pub(super) async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}/{}", self.endpoints.gateway(), path);
        let mut request = self.http.get(&url);
        if let Some(token) = &self.read_token {
            request = request.bearer_auth(token);
        }
        let response = request
            .send()
            .await
            .with_context(|| format!("failed to connect to package infrastructure at {url}"))?;
        decode_json(response, "package infrastructure", "GET", &url).await
    }

    pub(super) async fn get_work<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = self.work_url(path);
        let token = self
            .read_token
            .as_ref()
            .or(self.agent_token.as_ref())
            .or(self.controller_token.as_ref())
            .or(self.reviewer_token.as_ref())
            .or(self.build_token.as_ref())
            .or(self.release_token.as_ref())
            .or(self.rollout_token.as_ref())
            .context("package workflow read credential is unavailable")?;
        let response = self
            .http
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .with_context(|| format!("failed to connect to package workflow at {url}"))?;
        decode_json(response, "package workflow", "GET", &url).await
    }

    pub(super) async fn get_authenticated<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        token: Option<&str>,
        scope: &str,
    ) -> Result<T> {
        let token = token
            .or(self.controller_token.as_deref())
            .with_context(|| format!("package workflow {scope} credential is unavailable"))?;
        let url = self.work_url(path);
        let response = self
            .http
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .with_context(|| format!("failed to connect to package workflow at {url}"))?;
        decode_json(response, "package workflow", "GET", &url).await
    }

    pub(super) async fn post_work<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.post_authenticated(
            path,
            body,
            self.controller_token
                .as_deref()
                .or(self.agent_token.as_deref()),
            "agent or controller",
        )
        .await
    }

    pub(super) async fn post_source_sync<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.post_authenticated_with_timeout(
            path,
            body,
            self.controller_token
                .as_deref()
                .or(self.agent_token.as_deref()),
            "agent or controller",
            SOURCE_SYNC_TIMEOUT,
        )
        .await
    }

    pub(super) async fn post_release<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.post_authenticated(
            path,
            body,
            self.release_token
                .as_deref()
                .or(self.controller_token.as_deref()),
            "release",
        )
        .await
    }

    pub(super) async fn post_rollout_sync<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.post_authenticated_with_timeout(
            path,
            body,
            self.rollout_token
                .as_deref()
                .or(self.controller_token.as_deref()),
            "rollout",
            SOURCE_SYNC_TIMEOUT,
        )
        .await
    }

    pub(super) async fn post_authenticated<T, B>(
        &self,
        path: &str,
        body: &B,
        token: Option<&str>,
        scope: &str,
    ) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.post_authenticated_with_timeout(path, body, token, scope, REQUEST_TIMEOUT)
            .await
    }

    async fn post_authenticated_with_timeout<T, B>(
        &self,
        path: &str,
        body: &B,
        token: Option<&str>,
        scope: &str,
        timeout: Duration,
    ) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let token =
            token.with_context(|| format!("package workflow {scope} credential is unavailable"))?;
        let url = self.work_url(path);
        let response = self
            .http
            .post(&url)
            .timeout(timeout)
            .bearer_auth(token)
            .json(body)
            .send()
            .await
            .with_context(|| format!("failed to connect to package workflow at {url}"))?;
        decode_json(response, "package workflow", "POST", &url).await
    }

    pub(super) fn work_url(&self, path: &str) -> String {
        format!(
            "{}/work/{}",
            self.endpoints.gateway(),
            path.trim_start_matches('/')
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejection_detail_prefers_structured_errors_and_removes_control_characters() {
        assert_eq!(
            rejection_detail(br#"{"error":"invalid\nrelease\tstate"}"#),
            "invalid release state"
        );
    }

    #[test]
    fn rejection_detail_is_bounded_and_has_a_stable_empty_fallback() {
        assert_eq!(rejection_detail(b" \n\t"), "no error detail returned");
        assert_eq!(
            rejection_detail("x".repeat(ERROR_DETAIL_LIMIT + 50).as_bytes()).len(),
            ERROR_DETAIL_LIMIT
        );
    }
}
