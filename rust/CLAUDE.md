# Claude AI Development Notes

This document contains notes for Claude AI when working on this codebase.

## Running Integration Tests with Docker-in-Docker

Integration tests that create real Docker containers are marked with `#[ignore]` and require Docker to run.

### Setup Docker-in-Docker (in Claude's environment)

```bash
# Install Docker
curl -fsSL https://get.docker.com -o get-docker.sh && sudo sh get-docker.sh

# Start Docker daemon (disable iptables to avoid permission issues)
sudo dockerd --iptables=false --bridge=none > /tmp/dockerd.log 2>&1 &
sleep 5

# Fix socket permissions
sudo chmod 666 /var/run/docker.sock

# Verify Docker is working
docker info
```

### Run Integration Tests

```bash
# Run ignored tests (slow, creates real containers)
cargo test --test networking --features integration -- --ignored

# Run specific test
cargo test --test networking --features integration -- --ignored test_port_forwarding_single_port

# Run tests in parallel (faster)
cargo test --test networking --features integration -- --ignored --test-threads=3
```

### Known Limitations

- Docker-in-Docker may fail with `unshare: operation not permitted` due to nested containerization limits
- Tests take 60-120+ seconds when Docker properly builds images
- On developer machines with proper Docker, all tests should pass
