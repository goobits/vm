# API Reference

The vm-api service provides a REST API for managing hosted workspaces.

## Base URL

```
http://localhost:3000/api/v1
```

## Authentication

**Phase 1 (Development):** Use `x-user` header
```bash
curl -H "x-user: myusername" http://localhost:3000/api/v1/workspaces
```

**Phase 2 (Production):** Deploy oauth2-proxy or vm-auth-proxy in front of vm-api. The proxy will set `X-VM-User` header after GitHub OAuth verification.

## Endpoints

### List Workspaces

```
GET /api/v1/workspaces
```

Returns all workspaces owned by the authenticated user.

**Response:**
```json
[
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "name": "my-workspace",
    "owner": "alice",
    "template": "nodejs",
    "provider": "docker",
    "status": "running",
    "created_at": "2025-01-16T10:30:00Z",
    "updated_at": "2025-01-16T10:31:00Z",
    "ttl_seconds": 86400,
    "expires_at": "2025-01-17T10:30:00Z",
    "provider_id": "container-abc123",
    "connection_info": {
      "container_id": "abc123",
      "ssh_command": "vm ssh my-workspace"
    }
  }
]
```

### Create Workspace

```
POST /api/v1/workspaces
```

**Request body:**
```json
{
  "name": "my-workspace",
  "template": "nodejs",
  "ttl_seconds": 86400
}
```

**Response:** Same as workspace object above with `status="creating"`.

The workspace will be provisioned in the background. Poll the workspace status to see when it transitions to `"running"` or `"failed"`.

### Get Workspace

```
GET /api/v1/workspaces/{id}
```

Returns a single workspace.

### Delete Workspace

```
DELETE /api/v1/workspaces/{id}
```

Destroys the VM and removes the workspace record.

**Response:**
```json
{ "message": "Workspace deleted" }
```

## Workspace Statuses

- `creating` - Being provisioned (background task will update to running/failed)
- `running` - VM is running and accessible
- `stopped` - VM is stopped (Phase 2 feature)
- `failed` - Provisioning failed (see error_message field)

## Error Handling

The API returns standard HTTP status codes:

- `200 OK` - Request succeeded
- `401 Unauthorized` - Missing or invalid authentication
- `404 Not Found` - Workspace not found
- `500 Internal Server Error` - Server error

Error responses include a JSON body:
```json
{
  "error": "Error message description"
}
```

## Workspace Fields

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Unique workspace identifier (UUID) |
| `name` | string | User-friendly workspace name |
| `owner` | string | Username of workspace owner |
| `template` | string? | Template used to create workspace |
| `provider` | string | Provider type (e.g., "docker") |
| `status` | string | Current workspace status |
| `created_at` | string | ISO 8601 timestamp of creation |
| `updated_at` | string | ISO 8601 timestamp of last update |
| `ttl_seconds` | number? | Time-to-live in seconds |
| `expires_at` | string? | ISO 8601 timestamp when workspace expires |
| `provider_id` | string? | Provider-specific identifier (e.g., container ID) |
| `connection_info` | object? | Connection details (provider-specific) |
| `error_message` | string? | Error message if provisioning failed |
| `metadata` | object? | Additional metadata |

## Health Checks

- `GET /health` - Basic health check
- `GET /health/ready` - Readiness check (includes DB connectivity)

## Examples

### Create and Monitor a Workspace

```bash
# Create workspace
WORKSPACE_ID=$(curl -s -X POST http://localhost:3000/api/v1/workspaces \
  -H "x-user: alice" \
  -H "Content-Type: application/json" \
  -d '{"name": "test-workspace", "template": "nodejs", "ttl_seconds": 3600}' \
  | jq -r '.id')

# Poll status until running
while true; do
  STATUS=$(curl -s http://localhost:3000/api/v1/workspaces/$WORKSPACE_ID \
    -H "x-user: alice" \
    | jq -r '.status')

  echo "Status: $STATUS"

  if [ "$STATUS" = "running" ] || [ "$STATUS" = "failed" ]; then
    break
  fi

  sleep 2
done

# Get connection info
curl -s http://localhost:3000/api/v1/workspaces/$WORKSPACE_ID \
  -H "x-user: alice" \
  | jq '.connection_info'
```

### List All Workspaces

```bash
curl -H "x-user: alice" http://localhost:3000/api/v1/workspaces | jq
```

### Delete a Workspace

```bash
curl -X DELETE http://localhost:3000/api/v1/workspaces/$WORKSPACE_ID \
  -H "x-user: alice"
```

## Background Tasks

The vm-api service runs background tasks:

### Provisioner
- Monitors workspaces in `creating` status
- Provisions VMs using the configured provider
- Updates workspace status to `running` or `failed`
- Runs every 5 seconds

### Janitor
- Monitors workspaces with expired TTLs
- Automatically deletes expired workspaces
- Runs every 60 seconds

## Configuration

The API service uses SQLite for persistence. Database location:

- **Linux/macOS:** `~/.vm/api/vm.db`
- **Windows:** `%APPDATA%\vm\api\vm.db`

The service automatically creates and migrates the database on startup.
