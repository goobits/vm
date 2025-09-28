# vm-docker-registry

An intelligent, self-managing Docker registry cache for the VM tool that operates as invisible infrastructure, providing automatic caching and cleanup without any user intervention.

## Overview

The `vm-docker-registry` crate provides a transparent caching layer for Docker images used by VMs. It works like a browser cache - completely invisible to users while significantly improving Docker pull performance.

## Features

### 🚀 Zero-Configuration Operation
- **Automatic Startup**: Registry starts automatically when VMs need it
- **Transparent Configuration**: Auto-configures Docker daemon to use local mirror
- **Silent Operation**: No user-facing commands or configuration required
- **Lifecycle Management**: Tied to VM lifecycle - starts/stops based on VM needs

### 🤖 Intelligent Auto-Management
- **Background Health Monitoring**: Periodic health checks every 15 minutes
- **Self-Healing**: Automatic restart on failure (up to 3 attempts)
- **Age-Based Cleanup**: Removes images older than configured age (default: 30 days)
- **Size Management**: LRU eviction when approaching cache limits (default: 5GB)
- **Automatic Garbage Collection**: Frees disk space after cleanup operations

### 🐳 Docker Integration
- **Daemon Configuration**: Automatically updates daemon.json with registry mirror
- **Cross-Platform Support**: Works on Linux, macOS, and Windows
- **Backup & Restore**: Safe daemon configuration with automatic backups
- **Multiple Restart Methods**: Tries various approaches to reload Docker config

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                    VM Tool CLI                      │
└─────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────┐
│                  Service Manager                     │
│         (Manages registry lifecycle with VMs)        │
└─────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────┐
│              vm-docker-registry Crate               │
│                                                     │
│  ┌──────────────────┐  ┌──────────────────────┐   │
│  │   Auto-Manager   │  │  Docker Configurator │   │
│  │                  │  │                      │   │
│  │ • Health checks  │  │ • daemon.json mgmt  │   │
│  │ • Cache cleanup  │  │ • Mirror setup      │   │
│  │ • LRU eviction   │  │ • Daemon restart    │   │
│  │ • Self-healing   │  │                      │   │
│  └──────────────────┘  └──────────────────────┘   │
│                                                     │
│  ┌──────────────────────────────────────────────┐ │
│  │          Registry Server (nginx + registry)   │ │
│  │                                              │ │
│  │  • Proxy layer (port 5000)                  │ │
│  │  • Backend registry (port 5001)             │ │
│  │  • Docker Hub caching                       │ │
│  └──────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────┘
```

## Usage

### For VM Users

Simply enable the registry in your global configuration:

```yaml
# ~/.vm/config.yaml - Global configuration
services:
  docker_registry:
    enabled: true  # That's it! Registry is now auto-managed
```

The registry will:
- Start automatically when your VM starts
- Configure Docker to use the local cache
- Clean up old images periodically
- Stop when no VMs need it

### For Advanced Users

While the registry works with zero configuration, advanced users can customize its behavior:

```yaml
# ~/.vm/config.yaml - Advanced registry configuration
services:
  docker_registry:
    enabled: true
    max_cache_size_gb: 10        # Maximum cache size (default: 5)
    max_image_age_days: 60       # Keep images for 60 days (default: 30)
    cleanup_interval_hours: 2    # Run cleanup every 2 hours (default: 1)
    enable_lru_eviction: true     # Use LRU when cache is full (default: true)
    enable_auto_restart: true     # Auto-restart on failure (default: true)
    health_check_interval_minutes: 30  # Check health every 30min (default: 15)
```

## How It Works

### 1. Automatic Startup
When a VM is created and docker_registry is enabled in global config, the Service Manager:
- Starts the registry containers (nginx proxy + registry backend)
- Configures the Docker daemon to use `http://127.0.0.1:5000` as a mirror
- Spawns the auto-manager background task

### 2. Transparent Caching
All Docker pulls are automatically routed through the local registry:
- First pull: Image is fetched from Docker Hub and cached locally
- Subsequent pulls: Served instantly from local cache
- No manual configuration or commands required

### 3. Intelligent Cleanup
The auto-manager runs continuously in the background:
- **Health Monitoring**: Checks registry health every 15 minutes
- **Age-Based Cleanup**: Scans for images older than `max_image_age_days`
- **Size Management**: Triggers LRU eviction when approaching `max_cache_size_gb`
- **Garbage Collection**: Runs registry GC after deleting images

### 4. Automatic Shutdown
When the last VM using the registry is destroyed:
- Registry containers are stopped
- Docker daemon configuration is cleaned up
- Resources are freed

## API

### Core Types

```rust
/// Configuration for auto-management behavior
pub struct AutoConfig {
    pub max_cache_size_gb: u64,
    pub max_image_age_days: u32,
    pub cleanup_interval_hours: u32,
    pub enable_lru_eviction: bool,
    pub enable_auto_restart: bool,
    pub health_check_interval_minutes: u32,
}
```

### Main Functions

```rust
// Start the registry service
pub async fn start_registry() -> Result<()>

// Stop the registry service
pub async fn stop_registry() -> Result<()>

// Check if registry is running
pub async fn check_registry_running(port: u16) -> bool

// Start auto-manager background task
pub fn start_auto_manager() -> Result<()>

// Configure Docker daemon to use registry
pub async fn configure_docker_daemon(registry_url: &str) -> Result<()>
```

## Implementation Details

### Registry API Integration
The auto-manager uses the Docker Registry V2 API for cache management:
- `/v2/_catalog` - List all repositories
- `/v2/{repo}/tags/list` - List tags for a repository
- `/v2/{repo}/manifests/{tag}` - Get manifest with creation date
- `DELETE /v2/{repo}/manifests/{digest}` - Delete specific images

### Docker Daemon Configuration
Automatically manages `/etc/docker/daemon.json`:
```json
{
  "registry-mirrors": ["http://127.0.0.1:5000"],
  "insecure-registries": ["127.0.0.1:5000"]
}
```

### Container Management
Uses Docker Compose to manage registry containers:
- `vm-registry-proxy` - Nginx proxy on port 5000
- `vm-registry-backend` - Registry backend on port 5001

## Testing

```bash
# Run unit tests
cargo test --package vm-docker-registry

# Test categories:
# - Auto-manager logic (health checks, cleanup)
# - Docker configuration (daemon.json handling)
# - Registry API operations
# - Server lifecycle management
```

## Dependencies

- `tokio` - Async runtime for background tasks
- `reqwest` - HTTP client for registry API
- `chrono` - Date/time handling for age calculation
- `serde_json` - JSON parsing for daemon.json and manifests
- `docker-compose` - Container orchestration
- `nginx` - Proxy layer for caching

## License

Part of the VM tool project. See the main project LICENSE for details.