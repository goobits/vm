# vm-api

REST API service for managing hosted workspaces.

## Configuration

The service can be configured via environment variables:

- `VM_API_BIND` - Bind address (default: `0.0.0.0:3000`)
- `VM_API_DB_PATH` - SQLite database path (default: `~/.vm/api/vm.db`)
- `VM_API_JANITOR_INTERVAL` - Janitor interval in seconds (default: `300`)
- `VM_API_PROVISIONER_INTERVAL` - Provisioner check interval in seconds (default: `10`)

## Running Locally

```bash
# Build
cargo build --bin vm-api

# Run
cargo run --bin vm-api

# With custom config
VM_API_BIND=127.0.0.1:8080 \
VM_API_DB_PATH=/tmp/vm.db \
cargo run --bin vm-api
```

## Authentication

In production, deploy vm-auth-proxy or oauth2-proxy in front of vm-api.
The proxy should set `X-VM-User` header after GitHub OAuth verification.

For local development, use `x-user` header:
```bash
curl -H "x-user: myusername" http://localhost:3000/api/v1/workspaces
```

## Endpoints

- `GET /health` - Service health check
- `GET /health/ready` - Readiness check (includes DB connectivity)
- `GET /api/v1/workspaces` - List workspaces
- `POST /api/v1/workspaces` - Create workspace
- `GET /api/v1/workspaces/{id}` - Get workspace
- `DELETE /api/v1/workspaces/{id}` - Delete workspace

## Background Tasks

- **Janitor**: Runs every 5 minutes to cleanup expired workspaces (TTL enforcement)
- **Provisioner**: Runs every 10 seconds to process workspaces in "creating" status

## Database

SQLite database with automatic migrations on startup.
Backup created before each migration at: `{db_path}.backup.{timestamp}`
