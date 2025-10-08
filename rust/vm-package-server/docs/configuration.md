# ⚙️ Configuration Guide

Complete configuration options for the Goobits Package Server.

## Server Configuration

### Command Line Options

```bash
pkg-server start [OPTIONS]
```

| Option | Default | Description |
|--------|---------|-------------|
| `--host` | `0.0.0.0` | Bind address for the server |
| `--port` | `3080` | Port number |
| `--data` | `./data` | Storage directory for packages |

### Environment Variables

```bash
# Enable debug logging
RUST_LOG=debug pkg-server start

# Maximum verbosity
RUST_LOG=trace pkg-server start

# Authentication token (for publishing)
export PKG_SERVER_AUTH_TOKEN="your-secret-api-key"
```

## Authentication

The server supports optional Bearer token authentication to protect package upload endpoints.

### Configuration File

Create or edit `config.json` in the working directory:

```json
{
  "security": {
    "require_authentication": true,
    "api_keys": ["your-secret-api-key-here"],
    "allowed_publishers": []
  }
}
```

### Settings Explained

- **`require_authentication`**: Enable/disable auth (default: `false`)
- **`api_keys`**: Array of valid API keys for authentication
- **`allowed_publishers`**: Reserved for future use (username filtering)

### Using Authentication

#### Environment Variable (Recommended)
```bash
export PKG_SERVER_AUTH_TOKEN="your-secret-api-key"
pkg-server add  # Automatically uses the token
```

#### Manual Authentication
Include the Bearer token in the Authorization header:

```bash
# Python with twine
twine upload --repository-url http://localhost:3080/pypi/ \
  --username __token__ --password your-secret-api-key dist/*

# npm with auth token
npm config set //localhost:3080/npm/:_authToken your-secret-api-key
npm publish --registry http://localhost:3080/npm/

# curl example
curl -X POST http://localhost:3080/pypi/ \
  -H "Authorization: Bearer your-secret-api-key" \
  -F "content=@dist/package-1.0.0.whl"
```

### Testing Authentication

```bash
# Test with valid token
export PKG_SERVER_AUTH_TOKEN="your-secret-api-key"
pkg-server add

# Test with invalid token (should fail)
export PKG_SERVER_AUTH_TOKEN="wrong-key"
pkg-server add
# Error: Authentication failed: Invalid API key

# Test without token (should fail if auth required)
unset PKG_SERVER_AUTH_TOKEN
pkg-server add
# Error: Authentication required but no token provided
```

## Storage Configuration

### Directory Structure

The `--data` directory stores all packages:

```
data/
├── pypi/
│   └── packages/              # Python wheels and tarballs
│       ├── package-1.0.0.whl
│       └── package-1.0.0.whl.meta
├── npm/
│   ├── metadata/              # Package metadata JSON
│   │   └── package-name.json
│   └── tarballs/              # Package tarballs
│       └── package-1.0.0.tgz
└── cargo/
    ├── index/                 # Registry index files
    │   └── he/ll/hello-world
    └── crates/                # Crate files
        └── hello-world-0.1.0.crate
```

### Backup and Migration

```bash
# Backup all packages
tar -czf packages-backup.tar.gz data/

# Restore packages
tar -xzf packages-backup.tar.gz

# Move to new location
pkg-server start --data /new/path/to/packages
```

## Security Best Practices

### For Production Use

1. **Enable Authentication**
   ```json
   {
     "security": {
       "require_authentication": true,
       "api_keys": ["use-strong-random-keys-here"]
     }
   }
   ```

2. **Use HTTPS** (with reverse proxy)
   ```nginx
   server {
     listen 443 ssl;
     server_name packages.example.com;

     ssl_certificate /path/to/cert.pem;
     ssl_certificate_key /path/to/key.pem;

     location / {
       proxy_pass http://localhost:3080;
       proxy_set_header Host $host;
     }
   }
   ```

3. **Restrict Network Access**
   ```bash
   # Bind to localhost only
   pkg-server start --host 127.0.0.1

   # Or use firewall rules
   ufw allow from 192.168.1.0/24 to any port 3080
   ```

4. **Regular Backups**
   ```bash
   # Daily backup cron job
   0 2 * * * tar -czf /backups/packages-$(date +\%Y\%m\%d).tar.gz /data
   ```

## Performance Tuning

### For Large Deployments

```bash
# Increase file descriptors
ulimit -n 65536

# Run with optimized settings
RUST_LOG=warn pkg-server start \
  --data /fast-ssd/packages \
  --port 3080
```

### Docker Resource Limits

```yaml
services:
  package-server:
    image: goobits-pkg-server:latest
    deploy:
      resources:
        limits:
          cpus: '2'
          memory: 2G
        reservations:
          cpus: '1'
          memory: 1G
```

## Troubleshooting

### Check Server Status
```bash
pkg-server status
```

### View Logs
```bash
# Direct run
RUST_LOG=debug pkg-server start

# Docker
docker logs -f goobits-pkg-server-3080
```

### Common Issues

**Port Already in Use:**
```bash
# Find process using port
lsof -i :3080
# Or use different port
pkg-server start --port 3081
```

**Permission Denied:**
```bash
# Fix data directory permissions
chmod -R 755 data/
chown -R $USER:$USER data/
```

**Authentication Failures:**
```bash
# Check config file
cat config.json

# Verify token is set
echo $PKG_SERVER_AUTH_TOKEN

# Test with curl
curl -H "Authorization: Bearer your-token" \
  http://localhost:3080/api/pypi/packages/count
```