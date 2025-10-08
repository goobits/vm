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

### Environment Variables
- `DOCKER_BUILDKIT` - Enables BuildKit for faster builds.
- `COMPOSE_DOCKER_CLI_BUILD` - Use Docker CLI for Compose builds.

## Installation

This plugin is automatically installed with the VM tool. No additional installation required.

To verify availability:
```bash
vm config preset --list | grep docker
```

## Usage

Apply this preset to your project:
```bash
vm config preset docker
vm create
```

Or add to `vm.yaml`:
```yaml
preset: docker
```

## Configuration

### Additional Packages
```yaml
preset: docker
packages:
  pip:
    - new-python-package
  npm:
    - new-npm-package
```

## Common Use Cases

1. **Building a Docker Image**
   ```bash
   vm exec "docker build -t my-app ."
   ```

2. **Running a Compose Stack**
   ```bash
   vm exec "docker-compose up -d"
   ```

## Troubleshooting

### Issue: `docker: command not found` or `Cannot connect to the Docker daemon`
**Solution**: Ensure the Docker service is running on your host machine. For Docker Desktop, make sure the application is started. If on Linux, check the service status with `systemctl status docker`.

### Issue: Permission denied when running Docker commands
**Solution**: On Linux, you may need to add your user to the `docker` group: `sudo usermod -aG docker $USER` and then start a new shell session.

## Related Documentation

- [Configuration Guide](../../docs/user-guide/configuration.md)
- [Presets Overview](../../docs/user-guide/presets.md)
- [CLI Reference](../../docs/user-guide/cli-reference.md)

## License

MIT