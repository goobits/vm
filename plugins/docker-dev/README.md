# Docker Development Plugin

Container development and deployment tools for Docker workflows.

## What's Included

### System Packages
- `docker.io` - Docker engine
- `docker-compose` - Multi-container orchestration
- `docker-buildx` - Extended build capabilities

### Python Packages
- `docker-compose` - Python Docker Compose
- `docker` - Docker SDK for Python
- `portainer-py` - Portainer API client

### NPM Packages
- `dockerfile-language-server` - Dockerfile LSP

### Aliases
- `dc` → `docker-compose`
- `dps` → `docker ps`
- `di` → `docker images`
- `dlog` → `docker logs`
- `dexec` → `docker exec -it`

### Environment
- `DOCKER_BUILDKIT=1` - Enable BuildKit
- `COMPOSE_DOCKER_CLI_BUILD=1` - Use Docker CLI for Compose builds

## Installation

```bash
vm plugin install plugins/docker-dev
```

## Usage

```bash
vm config preset docker
```

## License

MIT