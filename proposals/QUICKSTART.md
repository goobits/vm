# VM API & Web UI - Quick Start Guide

**Last Updated: 2025-11-18**

## What's Been Built

Phase 1 of the VM API and Web UI is complete! You now have:
- REST API service for managing workspaces
- Web UI for point-and-click workspace management
- Full lifecycle controls (create, start, stop, restart, delete)
- Snapshot management
- Operations tracking

---

## Running the Full Stack (Local Development)

### Terminal 1: Start the API Service

```bash
cd rust
cargo run --bin vm-api
```

The API will:
- Start on `http://localhost:3121`
- Create database at `~/.vm/api/vm.db`
- Run background janitor (TTL cleanup) and provisioner

### Terminal 2: Start the Web UI

```bash
cd site
npm install          # First time only
npm run dev
```

The web UI will:
- Start on `http://localhost:3120`
- Proxy `/api` requests to the API on port 3121
- Be available at: **http://localhost:3120/app**

**Note**: Both ports (3120-3121) are in the `vm.yaml` exposed range (3120-3129)

---

## Using the Web UI

### 1. Open the App
Navigate to: **http://localhost:3120/app**

### 2. Create a Workspace
1. Click the **"+ New Workspace"** button
2. Fill in the form:
   - **Name**: e.g., `my-dev-env`
   - **Template**: Choose nodejs, python, rust, or go
   - **TTL**: Set time-to-live in hours (e.g., 24)
3. Click **"Create"**

The workspace will be created asynchronously. Status badge shows:
- 🟡 Creating
- 🟢 Running
- 🔴 Stopped
- ❌ Failed

### 3. Manage Lifecycle
- **Start**: Click the green "Start" button
- **Stop**: Click the red "Stop" button
- **Restart**: Click the "Restart" button
- **Delete**: Click the trash icon (requires confirmation)

### 4. Work with Snapshots
1. Click **"Snapshots"** button on any workspace
2. **Create snapshot**: Enter a name and click "Create"
3. **Restore**: Click "Restore" on any snapshot

### 5. View History
Click **"Operations"** button to see:
- Workspace creation events
- Snapshot operations
- Lifecycle changes

---

## Using the API Directly

### Authentication (Local Dev)
```bash
export USER="myusername"
```

### List Workspaces
```bash
curl -H "x-user: $USER" http://localhost:3121/api/v1/workspaces | jq
```

### Create Workspace
```bash
curl -X POST http://localhost:3121/api/v1/workspaces \
  -H "x-user: $USER" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "my-workspace",
    "template": "nodejs",
    "ttl_seconds": 86400
  }' | jq
```

### Get Workspace Details
```bash
WORKSPACE_ID="<id-from-create>"
curl -H "x-user: $USER" http://localhost:3121/api/v1/workspaces/$WORKSPACE_ID | jq
```

### Start Workspace
```bash
curl -X POST http://localhost:3121/api/v1/workspaces/$WORKSPACE_ID/start \
  -H "x-user: $USER" | jq
```

### Stop Workspace
```bash
curl -X POST http://localhost:3121/api/v1/workspaces/$WORKSPACE_ID/stop \
  -H "x-user: $USER" | jq
```

### Create Snapshot
```bash
curl -X POST http://localhost:3121/api/v1/workspaces/$WORKSPACE_ID/snapshots \
  -H "x-user: $USER" \
  -H "Content-Type: application/json" \
  -d '{"name": "my-backup"}' | jq
```

### List Snapshots
```bash
curl -H "x-user: $USER" \
  http://localhost:3121/api/v1/workspaces/$WORKSPACE_ID/snapshots | jq
```

### Restore Snapshot
```bash
SNAPSHOT_ID="<snapshot-id>"
curl -X POST http://localhost:3121/api/v1/workspaces/$WORKSPACE_ID/snapshots/$SNAPSHOT_ID/restore \
  -H "x-user: $USER" | jq
```

### Delete Workspace
```bash
curl -X DELETE http://localhost:3121/api/v1/workspaces/$WORKSPACE_ID \
  -H "x-user: $USER" | jq
```

### List Operations
```bash
# All operations for user
curl -H "x-user: $USER" http://localhost:3121/api/v1/operations | jq

# Operations for specific workspace
curl -H "x-user: $USER" \
  "http://localhost:3121/api/v1/operations?workspace_id=$WORKSPACE_ID" | jq
```

---

## Health Checks

```bash
# Basic health check
curl http://localhost:3121/health

# Readiness check (includes DB)
curl http://localhost:3121/health/ready
```

---

## Configuration

### API Service Environment Variables

```bash
VM_API_BIND=0.0.0.0:3121              # Bind address (default)
VM_API_DB_PATH=~/.vm/api/vm.db        # Database location
VM_API_JANITOR_INTERVAL=300           # TTL cleanup every 5 min
VM_API_PROVISIONER_INTERVAL=10        # Check provisioning every 10 sec
```

### Database Location
- **Linux/macOS**: `~/.vm/api/vm.db`
- **Windows**: `%APPDATA%\vm\api\vm.db`

Backups created automatically before migrations: `{db_path}.backup.{timestamp}`

---

## Production Deployment

### 1. Deploy vm-auth-proxy
Configure GitHub OAuth and deploy in front of both API and Web UI.

### 2. Configure Headers
Auth proxy must set:
- `X-VM-User`: GitHub username
- `X-VM-Email`: GitHub email

### 3. Run Services
```bash
# API service
VM_API_BIND=0.0.0.0:3121 vm-api

# Web UI (build for production)
cd site
npm run build
# Serve the build/ directory with your web server
```

---

## Port Configuration (vm.yaml)

The `vm.yaml` exposes ports **3120-3129**:
- **Port 3120**: Web UI (Vite dev server / production build)
- **Port 3121**: API service
- **Ports 3122-3129**: Available for future services

---

## What's NOT Implemented Yet (Phase 2/3)

### API
- ❌ `PATCH /api/v1/workspaces/{id}` - update metadata
- ❌ `POST /api/v1/workspaces/{id}/actions/rebuild`
- ❌ `POST /api/v1/workspaces/{id}/commands` - run commands
- ❌ `GET /api/v1/templates` - template discovery
- ❌ WebSocket endpoints for logs/events/shell
- ❌ OpenAPI specification

### Web UI
- ❌ Auto-refresh (manual refresh only)
- ❌ Log streaming viewer
- ❌ Interactive shell
- ❌ Templates loaded from API (hardcoded for now)
- ❌ Navigation link in site header
- ❌ Team/shared workspace visibility

---

## Troubleshooting

### API won't start
```bash
# Check database permissions
ls -la ~/.vm/api/

# Check if port is in use
lsof -i :3121
```

### Web UI can't connect to API
```bash
# Verify API is running
curl http://localhost:3121/health

# Check Vite proxy configuration (should proxy /api to localhost:3121)
```

### Workspaces stuck in "creating" status
```bash
# Check provisioner logs
# The provisioner runs every 10 seconds to process creating workspaces

# Check operations for errors
curl -H "x-user: $USER" http://localhost:3121/api/v1/operations | jq
```

### Port conflicts
```bash
# Check what's using the ports
lsof -i :3120
lsof -i :3121

# Use custom ports if needed
VM_API_BIND=0.0.0.0:3122 cargo run --bin vm-api
```

---

## File Locations

```
rust/
├── vm-api/              # API service binary
│   ├── src/
│   │   ├── routes/     # REST endpoints
│   │   ├── auth.rs     # Authentication middleware
│   │   ├── janitor.rs  # TTL cleanup
│   │   └── provisioner.rs  # Async workspace creation
│   └── tests/          # Integration tests
│
├── vm-orchestrator/     # Business logic library
│   ├── src/
│   │   ├── workspace.rs  # Workspace operations
│   │   ├── operation.rs  # Operations tracking
│   │   └── db.rs        # Database layer
│   └── migrations/      # SQL migrations

site/
├── src/
│   ├── routes/
│   │   └── app/         # Web UI route
│   ├── lib/
│   │   ├── components/  # UI components
│   │   │   ├── WorkspaceList.svelte
│   │   │   ├── CreateWorkspaceDrawer.svelte
│   │   │   ├── SnapshotManager.svelte
│   │   │   └── OperationsHistory.svelte
│   │   └── api/        # API client functions
│   └── vite.config.ts   # Proxy configuration (port 3120 → 3121)
```

---

## Next Steps

See individual proposal files for Phase 2/3 roadmap:
- `proposals/03a_api_service.proposal.md` - API features
- `proposals/03b_web_interface.proposal.md` - Web UI features
