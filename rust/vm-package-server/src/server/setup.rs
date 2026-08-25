use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use tracing::{debug, info, warn};

use crate::{
    config::Config,
    resolver::{ResolverService, CATALOG_REFRESH_INTERVAL},
    state::AppState,
    upstream::{UpstreamClient, UpstreamConfig},
    InternalRegistryClient,
};

pub(super) fn app_state(host: &str, port: u16, data_dir: &Path) -> Result<AppState> {
    vm_core::validation::validate_hostname(host)
        .map_err(|error| anyhow::anyhow!("Invalid host parameter: {error}"))?;
    if port == 0 {
        anyhow::bail!("invalid package registry port 0");
    }

    let data_dir = absolute_data_dir(data_dir)?;
    let internal_client = InternalRegistryClient::from_environment()?.map(Arc::new);
    let config = Arc::new(configure_security(
        host,
        std::env::var("PKG_SERVER_READ_TOKEN").ok().as_deref(),
        std::env::var("PKG_SERVER_PUBLISH_TOKEN").ok().as_deref(),
        internal_client.is_some(),
    )?);
    let resolver = Arc::new(ResolverService::from_environment(
        &data_dir,
        internal_client.clone(),
    ));
    if internal_client.is_some() {
        start_catalog_refresh(Arc::clone(&resolver));
    }

    Ok(AppState {
        data_dir,
        server_addr: format!("http://{host}:{port}"),
        upstream_client: Arc::new(UpstreamClient::new(UpstreamConfig::default())?),
        internal_client,
        config,
        resolver,
    })
}

fn absolute_data_dir(data_dir: &Path) -> Result<PathBuf> {
    match std::fs::canonicalize(data_dir) {
        Ok(path) => Ok(path),
        Err(_) => {
            std::fs::create_dir_all(data_dir)?;
            Ok(std::env::current_dir()?.join(data_dir))
        }
    }
}

fn start_catalog_refresh(resolver: Arc<ResolverService>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(CATALOG_REFRESH_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut unavailable = false;
        loop {
            interval.tick().await;
            match resolver.refresh().await {
                Ok(()) if unavailable => {
                    info!(
                        operation = "refresh_catalog",
                        outcome = "recovered",
                        "package catalog connection recovered"
                    );
                    unavailable = false;
                }
                Ok(()) => {}
                Err(error) if unavailable => {
                    debug!(
                        operation = "refresh_catalog",
                        error = %error,
                        "package catalog remains unavailable"
                    );
                }
                Err(error) => {
                    warn!(
                        operation = "refresh_catalog",
                        error = %error,
                        outcome = "using_snapshot",
                        "package catalog is unavailable"
                    );
                    unavailable = true;
                }
            }
        }
    });
}

pub(super) fn configure_security(
    host: &str,
    read_token: Option<&str>,
    publish_token: Option<&str>,
    read_only: bool,
) -> Result<Config> {
    let mut config = Config::default();
    let read_token = read_token.filter(|token| !token.trim().is_empty());
    let publish_token = publish_token.filter(|token| !token.trim().is_empty());

    if !is_loopback_host(host) {
        if read_token.is_none() && !read_only {
            anyhow::bail!(
                "Refusing to bind package server to non-loopback host '{host}' without a read token"
            );
        }
        if !read_only && publish_token.is_none() {
            anyhow::bail!(
                "Refusing to bind writable package server to non-loopback host '{host}' without a separate publish token"
            );
        }
    }

    if read_token.is_some() || publish_token.is_some() {
        config.security.require_authentication = true;
        config.security.read_keys = read_token.map(str::to_string).into_iter().collect();
        config.security.publish_keys = publish_token.map(str::to_string).into_iter().collect();
    }
    Ok(config)
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}
