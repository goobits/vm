# vm-api

REST API service for managing hosted workspaces.

## Configuration

The service can be configured via environment variables:

- `VM_API_BIND` - Bind address (default: `0.0.0.0:3121`)
- `VM_API_DB_PATH` - SQLite database path (default: `~/.vm/api/vm.db`)
- `VM_API_JANITOR_INTERVAL` - Janitor interval in seconds (default: `300`)
- `VM_API_PROVISIONER_INTERVAL` - Provisioner check interval in seconds (default: `10`)

**Note**: Port 3121 is within the vm.yaml exposed port range (3120-3129). Port 3120 is used by the web UI.

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
curl -H "x-user: myusername" http://localhost:3121/api/v1/workspaces
```

## Endpoints

### Health
- `GET /health` - Service health check
- `GET /health/ready` - Readiness check (includes DB connectivity)

### Workspaces
- `GET /api/v1/workspaces` - List workspaces for authenticated user
- `POST /api/v1/workspaces` - Create workspace
- `GET /api/v1/workspaces/{id}` - Get workspace details
- `DELETE /api/v1/workspaces/{id}` - Delete workspace

### Operations
- `GET /api/v1/operations` - List operations (supports filters: workspace_id, type, status)
- `GET /api/v1/operations/{id}` - Get operation details

## Background Tasks

- **Janitor**: Runs every 5 minutes to cleanup expired workspaces (TTL enforcement)
- **Provisioner**: Runs every 10 seconds to process workspaces in "creating" status

## Database

SQLite database with automatic migrations on startup.

### Automatic Migrations

The service automatically runs database migrations on startup:

1. **Backup**: Before running migrations, the database is automatically backed up
2. **Migration**: Migrations are applied using sqlx migrations
3. **Verification**: The service logs successful migration completion

Backup files are created at: `{db_path}.backup.{timestamp}`

### Manual Database Management

#### Viewing Backup Files

```bash
# List backups
ls -lh ~/.vm/api/*.backup.*

# Most recent backup
ls -t ~/.vm/api/*.backup.* | head -1
```

#### Restoring from Backup

If you need to roll back a migration:

```bash
# 1. Stop the vm-api service
pkill vm-api

# 2. Restore from backup
cp ~/.vm/api/vm.db.backup.{timestamp} ~/.vm/api/vm.db

# 3. Restart the service
vm-api
```

#### Migration Safety

- Backups are created automatically before each migration
- Backups are timestamped (Unix timestamp)
- Migrations are idempotent and safe to run multiple times
- Schema changes are tested in integration tests before deployment

### Database Location

Default: `~/.vm/api/vm.db`

Override with: `VM_API_DB_PATH` environment variable
