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
    crate::validation::validate_docker_port(port)
        .map_err(|error| anyhow::anyhow!("Invalid port parameter: {error}"))?;

    let data_dir = absolute_data_dir(data_dir)?;
    let internal_client = InternalRegistryClient::from_environment()?.map(Arc::new);
    let config = Arc::new(configure_security(
        host,
        std::env::var("PKG_SERVER_READ_TOKEN").ok().as_deref(),
        std::env::var("PKG_SERVER_PUBLISH_TOKEN")
            .ok()
            .or_else(|| std::env::var("PKG_SERVER_AUTH_TOKEN").ok())
            .as_deref(),
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
                    info!("package catalog connection recovered");
                    unavailable = false;
                }
                Ok(()) => {}
                Err(error) if unavailable => {
                    debug!(error = %error, "package catalog remains unavailable");
                }
                Err(error) => {
                    warn!(error = %error, "package catalog is unavailable; using the last known snapshot");
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

pub(super) fn client_script(registry: &str, port: u16) -> String {
    let server_url = format!("http://$(hostname -I | cut -d' ' -f1):{port}");

    match registry {
        "npm" => format!(
            r#"#!/bin/bash
# Goobits Package Server - NPM Setup Script
# This script configures npm to use your private package registry

echo "🔧 Configuring npm to use Goobits Package Server..."
echo "📡 Registry URL: {server_url}/npm/"

# Set npm registry
npm config set registry {server_url}/npm/

echo "✅ npm configured successfully!"
echo ""
echo "📋 Useful commands:"
echo "   npm whoami          # Check current user"
echo "   npm config list     # View configuration"
echo "   npm config set registry https://registry.npmjs.org/  # Reset to default"
echo ""
echo "🚀 You can now install packages from your private registry!"
"#
        ),
        "pypi" => format!(
            r#"#!/bin/bash
# Goobits Package Server - PyPI Setup Script
# This script configures pip to use your private package registry

echo "🔧 Configuring pip to use Goobits Package Server..."
echo "📡 Registry URL: {server_url}/pypi/simple/"

# Create pip config directory
mkdir -p ~/.config/pip

# Configure pip
cat > ~/.config/pip/pip.conf << EOF
[global]
index-url = {server_url}/pypi/simple/
trusted-host = $(echo {server_url} | cut -d'/' -f3 | cut -d':' -f1)
EOF

echo "✅ pip configured successfully!"
echo ""
echo "📋 Useful commands:"
echo "   pip config list     # View configuration"
echo "   pip install --index-url https://pypi.org/simple/ <package>  # Install from PyPI"
echo ""
echo "🚀 You can now install packages from your private registry!"
"#
        ),
        "cargo" => format!(
            r#"#!/bin/bash
# Goobits Package Server - Cargo Setup Script
# This script configures cargo to use your private package registry

echo "🔧 Configuring cargo to use Goobits Package Server..."
echo "📡 Registry URL: {server_url}/cargo/"

# Create cargo config directory
mkdir -p ~/.cargo

# Configure cargo
cat > ~/.cargo/config.toml << EOF
[registries]
goobits = {{ index = "{server_url}/cargo/" }}

[source.crates-io]
replace-with = "goobits"

[source.goobits]
registry = "{server_url}/cargo/"
EOF

echo "✅ cargo configured successfully!"
echo ""
echo "📋 Useful commands:"
echo "   cargo search <package>    # Search for packages"
echo "   cargo install <package>   # Install a package"
echo ""
echo "🚀 You can now install packages from your private registry!"
"#
        ),
        _ => {
            warn!(registry, "unknown registry type requested");
            format!(
                r#"#!/bin/bash
# Goobits Package Server - Setup Script
# Unknown registry type: {registry}

echo "❌ Unknown registry type: {registry}"
echo "📋 Supported registries: npm, pypi, cargo"
echo ""
echo "🔧 Usage examples:"
echo "   curl {server_url}/setup.sh?registry=npm | bash"
echo "   curl {server_url}/setup.sh?registry=pypi | bash"
echo "   curl {server_url}/setup.sh?registry=cargo | bash"
"#
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::client_script;

    #[test]
    fn pypi_script_writes_only_the_canonical_config() {
        let script = client_script("pypi", 3080);

        assert!(script.contains("~/.config/pip/pip.conf"));
        assert!(!script.contains("~/.pip"));
    }
}
