# Docker Registry Integration Proposal v2

## Overview
Add local Docker registry to cache images and eliminate redundant downloads when creating multiple VMs.

## Current State
- ❌ No Docker registry implementation
- ❌ Each VM pulls identical images from Docker Hub repeatedly

## Problem
Massive bandwidth waste on identical Docker image pulls:
```bash
vm create frontend  # Pulls node:20 (400MB)
vm create backend   # Pulls node:20 AGAIN (400MB)
vm create api       # Pulls node:20 AGAIN (400MB)
# Total: 1.2GB for same image
```

## Solution
Host-level Docker registry using nginx proxy + registry:2 for true pull-through caching.

### Configuration
```yaml
# vm.yaml
docker_registry: true  # Enable connection to host Docker registry
```

### CLI Interface
```bash
vm registry start              # Start Docker registry on host (port 5000)
vm registry stop               # Stop registry
vm registry status             # Check status and storage usage
vm registry gc                 # Manual garbage collection
vm registry config show       # Show configuration
```

### Implementation Tasks

1. **Add registry commands** (`rust/vm/src/commands/registry.rs`):
   ```rust
   pub enum RegistryCommand {
       Start,
       Stop,
       Status,
       Gc { force: bool },
       Config(ConfigCommand),
   }
   ```

2. **Registry deployment architecture**:
   ```bash
   # Nginx proxy for pull-through caching
   docker run -d --name vm-registry-proxy \
     -p 5000:80 \
     -v ~/.vm/registry/nginx.conf:/etc/nginx/nginx.conf \
     nginx:alpine

   # Registry backend for storage
   docker run -d --name vm-registry-storage \
     -v ~/.vm/registry/data:/var/lib/registry \
     registry:2
   ```

3. **Nginx configuration for pull-through**:
   ```nginx
   upstream registry {
       server 127.0.0.1:5001;
   }
   upstream dockerhub {
       server registry-1.docker.io:443;
   }

   server {
       location / {
           proxy_pass http://registry;
           error_page 404 = @dockerhub;
       }

       location @dockerhub {
           proxy_pass https://dockerhub;
           proxy_intercept_errors off;
       }
   }
   ```

4. **VM Docker configuration** (`rust/vm-provider/src/*/provisioning.rs`):
   - Check if `docker_registry: true` in config
   - Update `/etc/docker/daemon.json`:
     ```json
     {
       "registry-mirrors": ["http://host.docker.internal:5000"],
       "insecure-registries": ["host.docker.internal:5000"]
     }
     ```
   - Restart Docker daemon in VM

5. **Auto-start logic**:
   - `vm create` with Docker service checks if registry running on port 5000
   - If not running and `docker_registry: true`, prompt: "Start Docker registry? [Y/n]"
   - Auto-execute `vm registry start` if user confirms

6. **Lifecycle management**:
   - Health checks for both nginx proxy and registry backend
   - Automatic restart on failure
   - Graceful cleanup on `vm registry stop`

### Expected Results
- **First VM**: Normal Docker Hub download speed (populates cache)
- **Subsequent VMs**: Near-instant image retrieval from local cache
- **Bandwidth savings**: 80-95% for teams reusing base images
- **Offline capability**: Previously pulled images available without internet

### Technical Notes
- Uses nginx proxy for true pull-through functionality (registry:2 alone can't do this)
- Uses Docker's standard registry port (5000)
- Registry persists between host reboots
- Transparent to existing Docker commands
- Compatible with both Docker provider and future providers
- No impact on VM port ranges (uses fixed host port)

### Cleanup Policy
```yaml
# Default configuration
docker_registry:
  enabled: true
  gc_policy: "30d"        # Delete unused images after 30 days
  max_size: "50GB"        # Maximum registry size
  lru_cleanup: true       # Remove least-recently-used first
```

### Success Criteria
- [ ] Docker registry pull-through caching working (nginx + registry:2)
- [ ] VMs automatically configured to use local registry when `docker_registry: true`
- [ ] Image bandwidth savings demonstrated (80%+ reduction)
- [ ] Registry survives host reboots with persistent storage
- [ ] `vm registry gc` cleanup functionality working