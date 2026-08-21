use std::time::Duration;

use anyhow::{Context, Result};
use serde::Serialize;

use super::{PackageInfrastructureClient, REQUEST_TIMEOUT};

const SOURCE_SYNC_TIMEOUT: Duration = Duration::from_secs(60 * 60);

impl PackageInfrastructureClient {
    pub(super) async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}/{}", self.endpoints.gateway(), path);
        let mut request = self.http.get(&url);
        if let Some(token) = &self.read_token {
            request = request.bearer_auth(token);
        }
        request
            .send()
            .await
            .with_context(|| format!("failed to connect to package infrastructure at {url}"))?
            .error_for_status()
            .with_context(|| format!("package infrastructure rejected GET {url}"))?
            .json()
            .await
            .with_context(|| format!("package infrastructure returned invalid JSON from {url}"))
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
        self.http
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .with_context(|| format!("failed to connect to package workflow at {url}"))?
            .error_for_status()
            .with_context(|| format!("package workflow rejected GET {url}"))?
            .json()
            .await
            .with_context(|| format!("package workflow returned invalid JSON from {url}"))
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
        self.http
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .with_context(|| format!("failed to connect to package workflow at {url}"))?
            .error_for_status()
            .with_context(|| format!("package workflow rejected GET {url}"))?
            .json()
            .await
            .with_context(|| format!("package workflow returned invalid JSON from {url}"))
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
        self.http
            .post(&url)
            .timeout(timeout)
            .bearer_auth(token)
            .json(body)
            .send()
            .await
            .with_context(|| format!("failed to connect to package workflow at {url}"))?
            .error_for_status()
            .with_context(|| format!("package workflow rejected POST {url}"))?
            .json()
            .await
            .with_context(|| format!("package workflow returned invalid JSON from {url}"))
    }

    pub(super) fn work_url(&self, path: &str) -> String {
        format!(
            "{}/work/{}",
            self.endpoints.gateway(),
            path.trim_start_matches('/')
        )
    }
}
