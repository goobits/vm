//! Configuration generation for Docker registry service

use crate::types::RegistryConfig;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use tera::{Context as TeraContext, Tera};

/// Generate nginx configuration for pull-through caching
pub fn generate_nginx_config(config: &RegistryConfig) -> Result<String> {
    let template = r#"
events {
    worker_connections 1024;
}

http {
    upstream registry {
        server {{ backend_host }}:{{ backend_port }};
    }

    upstream dockerhub {
        server registry-1.docker.io:443;
    }

    # Proxy cache configuration
    proxy_cache_path /var/cache/nginx levels=1:2 keys_zone=registry_cache:10m
                     max_size=10g inactive=60m use_temp_path=off;

    server {
        listen 80;
        server_name _;

        # Enable proxy cache
        proxy_cache registry_cache;
        proxy_cache_valid 200 1h;
        proxy_cache_use_stale error timeout updating http_500 http_502 http_503 http_504;
        proxy_cache_lock on;

        # Increase timeouts for large image pulls
        proxy_connect_timeout 300s;
        proxy_send_timeout 300s;
        proxy_read_timeout 300s;
        send_timeout 300s;

        # Buffer settings for large files
        proxy_buffering on;
        proxy_buffer_size 4k;
        proxy_buffers 8 4k;
        proxy_max_temp_file_size 2048m;

        # Registry v2 API
        location /v2/ {
            # Try local registry first
            proxy_pass http://registry;
            proxy_set_header Host $http_host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;

            # On 404, try Docker Hub
            error_page 404 = @dockerhub;
        }

        # Fallback to Docker Hub
        location @dockerhub {
            proxy_pass https://registry-1.docker.io;
            proxy_set_header Host registry-1.docker.io;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;

            # Don't cache errors from upstream
            proxy_intercept_errors off;

            # Store successful responses in local registry
            # This is handled by the registry backend when images are pulled
        }

        # Health check endpoint
        location /health {
            access_log off;
            return 200 "healthy\n";
            add_header Content-Type text/plain;
        }

        # Disable logging for successful pulls to reduce noise
        location ~* \.(blob|manifest) {
            proxy_pass http://registry;
            proxy_set_header Host $http_host;
            access_log off;
            error_page 404 = @dockerhub;
        }
    }
}
"#;

    let mut tera = Tera::new("/dev/null/*").expect("should create tera instance");
    let mut context = TeraContext::new();
    context.insert("backend_host", &config.host);
    context.insert("backend_port", &config.backend_port);

    tera.render_str(template, &context)
        .context("Failed to render nginx configuration")
}

/// Generate Docker registry configuration
pub fn generate_registry_config(config: &RegistryConfig) -> Result<String> {
    let config_yaml = format!(
        r#"version: 0.1
log:
  level: {}
storage:
  filesystem:
    rootdirectory: /var/lib/registry
  cache:
    blobdescriptor: inmemory
  delete:
    enabled: true
http:
  addr: :5000
  host: http://{}:{}
  relativeurls: false
  draintimeout: 60s
health:
  storagedriver:
    enabled: true
    interval: 10s
    threshold: 3
proxy:
  remoteurl: https://registry-1.docker.io
"#,
        if config.debug { "debug" } else { "info" },
        config.host,
        config.backend_port
    );

    Ok(config_yaml)
}

/// Generate Docker Compose configuration for the registry
pub fn generate_docker_compose_config(config: &RegistryConfig, data_dir: &str) -> Result<String> {
    let compose_yaml = format!(
        r#"version: '3.8'
services:
  registry:
    image: registry:2
    container_name: vm-registry-backend
    restart: unless-stopped
    ports:
      - "{}:{}:5000"
    volumes:
      - "{}:/var/lib/registry"
      - "./registry-config.yml:/etc/docker/registry/config.yml"
    environment:
      - REGISTRY_STORAGE_DELETE_ENABLED=true
    networks:
      - registry-network

  proxy:
    image: nginx:alpine
    container_name: vm-registry-proxy
    restart: unless-stopped
    ports:
      - "{}:{}:80"
    volumes:
      - "./nginx.conf:/etc/nginx/nginx.conf:ro"
      - "nginx-cache:/var/cache/nginx"
    depends_on:
      - registry
    networks:
      - registry-network

volumes:
  nginx-cache:

networks:
  registry-network:
    driver: bridge
"#,
        config.host, config.backend_port, data_dir, config.host, config.registry_port
    );

    Ok(compose_yaml)
}

/// Write configuration files to data directory
pub fn write_config_files(config: &RegistryConfig, data_dir: &Path) -> Result<()> {
    // Ensure data directory exists
    fs::create_dir_all(data_dir).context("Failed to create data directory")?;

    // Generate configurations
    let nginx_config = generate_nginx_config(config)?;
    let registry_config = generate_registry_config(config)?;
    let compose_config = generate_docker_compose_config(config, &data_dir.to_string_lossy())?;

    // Write configuration files
    fs::write(data_dir.join("nginx.conf"), nginx_config)
        .context("Failed to write nginx configuration")?;

    fs::write(data_dir.join("registry-config.yml"), registry_config)
        .context("Failed to write registry configuration")?;

    fs::write(data_dir.join("docker-compose.yml"), compose_config)
        .context("Failed to write docker-compose configuration")?;

    Ok(())
}

/// Get default registry data directory
pub fn get_registry_data_dir() -> Result<std::path::PathBuf> {
    let home_dir = dirs::home_dir().context("Failed to get home directory")?;
    Ok(home_dir.join(".vm").join("registry"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_generate_nginx_config() {
        let config = RegistryConfig::default();
        let nginx_config = generate_nginx_config(&config).expect("should generate nginx config");

        assert!(nginx_config.contains("upstream registry"));
        assert!(nginx_config.contains("upstream dockerhub"));
        assert!(nginx_config.contains("127.0.0.1:5001"));
        assert!(nginx_config.contains("location /v2/"));
    }

    #[test]
    fn test_generate_registry_config() {
        let config = RegistryConfig::default();
        let registry_config =
            generate_registry_config(&config).expect("should generate registry config");

        assert!(registry_config.contains("version: 0.1"));
        assert!(registry_config.contains("filesystem:"));
        assert!(registry_config.contains("enabled: true"));
        assert!(registry_config.contains("remoteurl: https://registry-1.docker.io"));
    }

    #[test]
    fn test_generate_docker_compose_config() {
        let config = RegistryConfig::default();
        let data_dir = "/test/data";
        let compose_config = generate_docker_compose_config(&config, data_dir)
            .expect("should generate docker compose config");

        assert!(compose_config.contains("version: '3.8'"));
        assert!(compose_config.contains("vm-registry-backend"));
        assert!(compose_config.contains("vm-registry-proxy"));
        assert!(compose_config.contains("5000:80"));
        assert!(compose_config.contains("/test/data:/var/lib/registry"));
    }

    #[test]
    fn test_write_config_files() {
        let temp_dir = TempDir::new().expect("should create temp dir");
        let config = RegistryConfig::default();

        write_config_files(&config, temp_dir.path()).expect("should write config files");

        assert!(temp_dir.path().join("nginx.conf").exists());
        assert!(temp_dir.path().join("registry-config.yml").exists());
        assert!(temp_dir.path().join("docker-compose.yml").exists());

        // Verify nginx config content
        let nginx_content =
            fs::read_to_string(temp_dir.path().join("nginx.conf")).expect("should read nginx.conf");
        assert!(nginx_content.contains("upstream registry"));
    }
}
