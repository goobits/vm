# VM Orchestrator - Quick Start Guide

## 🚀 Starting the Service

### 1. Start the API Server

```bash
# From /workspace/rust directory
cd /workspace/rust

# Start vm-api (with default settings)
cargo run --bin vm-api

# Or with custom configuration
VM_API_BIND=0.0.0.0:3000 \
VM_API_DB_PATH=~/.vm/api/vm.db \
cargo run --bin vm-api
```

The service will:
- Start on `http://0.0.0.0:3000` (default)
- Run database migrations automatically
- Start background tasks (provisioner, janitor)
- Log activity to console

### 2. Access the UI

Open your browser to:
```
http://localhost:3000/app
```

You'll see the Workspaces dashboard with all your development environments.

---

## 📋 Using the Features

### ✨ Creating a Workspace

1. Click the **"+ New Workspace"** button (top right)
2. Fill in the form:
   - **Name**: `my-project` (required)
   - **Template**: Choose `nodejs`, `python`, or `rust` (optional)
   - **Repository URL**: Git repo to clone (optional)
   - **TTL**: Time-to-live in hours (optional, default: no expiration)
3. Click **Create**
4. Watch the workspace appear with status `creating`

**What happens behind the scenes:**
- Workspace row appears immediately with "Provisioning..." badge
- Background provisioner picks it up within 10 seconds
- Docker container is created with selected template
- Status changes: `creating` → `running`
- Connection info appears (container ID, SSH command)

---

### 🎮 Lifecycle Controls (Start/Stop/Restart)

Each workspace row has action buttons based on its status:

#### When Workspace is **Running**:
- **Stop** button (orange) - Stops the container
- **Restart** button (blue) - Stops then starts the container
- **Delete** button (red) - Permanently deletes the workspace

#### When Workspace is **Stopped**:
- **Start** button (green) - Starts the container
- **Delete** button (red) - Permanently deletes the workspace

**Example workflow:**
1. Click **Stop** on a running workspace
   - Button shows "Stopping..." while in progress
   - Status badge changes to "Disconnected"
   - SSH command becomes unavailable

2. Click **Start** to bring it back
   - Button shows "Starting..." while in progress
   - Status badge changes to "Connected"
   - Fresh SSH command and container ID appear

3. Click **Restart** if something is stuck
   - Performs stop + start cycle
   - Regenerates connection metadata
   - Useful after configuration changes

---

### 📊 Activity / Operations Tracking

**Access:** Click the **"Activity"** button (cyan) on any workspace

**What you see:**
- Complete operation history for that workspace
- Most recent operations at the top
- Each operation shows:
  - **Icon**: 🔨 Create, ▶️ Start, ⏹️ Stop, 🔄 Restart, 📸 Snapshot, 🗑️ Delete
  - **Status Badge**:
    - 🟡 Pending (waiting to start)
    - 🔵 Running (in progress, with pulse animation)
    - 🟢 Success (completed)
    - 🔴 Failed (with error message)
  - **Duration**: How long it took (e.g., "45s", "2m 15s")
  - **Timestamp**: When it started (e.g., "5m ago", "Just now")

**Real-time updates:**
- Automatically refreshes every 3 seconds while operations are active
- Click **"↻ Refresh"** to manually update

**Error visibility:**
```
Example error display:
⚠️ Failed to start container: port 8080 already in use
```

**Use cases:**
- **Debug provisioning**: See why a workspace is stuck in "creating"
- **Track long operations**: Monitor snapshot creation progress
- **Audit trail**: See who did what and when
- **Error diagnosis**: Get detailed error messages from failed operations

---

### 📸 Snapshot Management

**Access:** Click the **"Snapshots"** button (purple) on any workspace

#### Creating a Snapshot

1. Click **"Create Snapshot"**
2. Enter a descriptive name:
   - `before-refactor`
   - `working-state-2024-11-17`
   - `pre-deployment`
3. Click **Create**
4. Watch in **Activity** tab:
   - See "Snapshot" operation appear as Running
   - Docker commits the container to an image
   - Image saved to `/tmp/vm-snapshots/{name}.tar`
   - Status changes to Success
   - Snapshot appears in list with size

#### Restoring from a Snapshot

1. Open **Snapshots** for the workspace
2. Find the snapshot you want
3. Click **"Restore"** button
4. Confirm the action (workspace will be stopped)
5. Watch in **Activity** tab:
   - See "SnapshotRestore" operation Running
   - Container is stopped
   - Snapshot tar is loaded
   - New container started from snapshot
   - Connection info regenerated
   - Status changes to Success

**Important:**
- Restoring replaces the current workspace state
- Workspace will be stopped during restore
- Connection info (container ID, SSH) is regenerated
- Can take 30s-2m depending on image size

**Use cases:**
- **Before risky changes**: Snapshot before major refactoring
- **Known good states**: Save working configurations
- **Rollback**: Quickly revert to previous state
- **Testing**: Try changes, then restore if they don't work

---

### 🔗 Connection Metadata & Quick Actions

**Location:** The "Connection" column in the workspace table

For each **running** workspace, you'll see:

#### Container ID
```
Container: a1b2c3d4e5f6
📋 [copy button]
```
- Click 📋 to copy full container ID to clipboard
- Useful for `docker exec`, `docker logs`, etc.

#### SSH Command
```
SSH: vm ssh my-project
📋 [copy button]
```
- Click 📋 to copy the SSH command
- Paste into terminal to connect
- Uses the `vm` CLI tool

#### Connection Status Badge
- **● Connected** (green) - Workspace is running
- **○ Disconnected** (gray) - Workspace is stopped
- **◌ Provisioning...** (yellow) - Workspace is being created

#### Quick Action
```
🔗 Open in Claude Code
```
- Click to open workspace in Claude Code
- Uses `vscode://` protocol
- Only appears for running workspaces

**Example workflow:**
1. Workspace is running
2. Click 📋 next to SSH command
3. Paste into terminal: `vm ssh my-project`
4. You're now inside the container
5. Or click "Open in Claude Code" for IDE access

---

## 🔄 Auto-Refresh Behavior

The UI automatically refreshes to keep data current:

- **Workspace list**: Refreshes every 10 seconds
- **Operations (when Activity is open)**: Refreshes every 3 seconds if operations are Pending/Running
- **Manual refresh**: Click any refresh button (↻) to force update

---

## 🎯 Common Workflows

### Workflow 1: Create and Connect to Workspace

1. Click **"+ New Workspace"**
2. Enter name: `my-app`, template: `nodejs`
3. Wait ~30 seconds for provisioning
4. Watch **Activity** tab to see progress
5. Once status is "Connected":
   - Click 📋 next to SSH command
   - Run `vm ssh my-app` in terminal
   - You're in!

### Workflow 2: Snapshot Before Major Changes

1. Find your workspace in the list
2. Click **"Snapshots"** button
3. Click **"Create Snapshot"**
4. Name it: `before-refactor`
5. Wait for "Snapshot" operation to complete in **Activity**
6. Make your risky changes
7. If things break:
   - Click **"Snapshots"** again
   - Find `before-refactor`
   - Click **"Restore"**
   - Watch **Activity** for completion
   - You're back to the known good state!

### Workflow 3: Debugging Failed Provisioning

1. Create workspace
2. Status shows "Failed" with ⚠️
3. Click **"Activity"** button
4. Look for "Create" operation with status Failed
5. Read the error message:
   ```
   ⚠️ Failed to pull image nodejs:20 - network timeout
   ```
6. Fix the issue (check network, Docker daemon)
7. Delete and recreate workspace

### Workflow 4: Managing Resources

1. See which workspaces are running (green "Connected")
2. Stop unused workspaces to free resources:
   - Click **Stop** on each
   - Watch status change to "Disconnected"
3. Restart when needed:
   - Click **Start**
   - Fresh connection info appears
4. Delete old workspaces:
   - Click **Delete** (red button)
   - Confirm deletion
   - Watch "Delete" operation complete in Activity

---

## 🔍 Monitoring & Debugging

### Check Operation Status

```bash
# Via API
curl -H "x-user: testuser" http://localhost:3000/api/v1/operations

# Filter by workspace
curl -H "x-user: testuser" \
  "http://localhost:3000/api/v1/operations?workspace_id=abc123"
```

### Check Workspace Details

```bash
curl -H "x-user: testuser" \
  http://localhost:3000/api/v1/workspaces/{workspace-id}
```

### View Logs

```bash
# API logs (console)
cargo run --bin vm-api

# Docker container logs
docker logs {container-id}
```

### Database Location

```bash
# Default location
~/.vm/api/vm.db

# View with sqlite3
sqlite3 ~/.vm/api/vm.db "SELECT * FROM workspaces;"
sqlite3 ~/.vm/api/vm.db "SELECT * FROM operations ORDER BY started_at DESC LIMIT 10;"
```

---

## ⚙️ Configuration

### Environment Variables

```bash
# API bind address (default: 0.0.0.0:3000)
export VM_API_BIND=127.0.0.1:8080

# Database path (default: ~/.vm/api/vm.db)
export VM_API_DB_PATH=/custom/path/vm.db

# Janitor interval - TTL cleanup (default: 300 seconds)
export VM_API_JANITOR_INTERVAL=600

# Provisioner interval - workspace creation (default: 10 seconds)
export VM_API_PROVISIONER_INTERVAL=5
```

### Authentication (Development)

Currently uses mock auth with `x-user` header:

```bash
# All API requests
curl -H "x-user: yourname" http://localhost:3000/api/v1/workspaces

# UI automatically uses 'testuser'
# See: site/src/lib/api/workspaces.ts
```

**Production**: Deploy with `oauth2-proxy` or `vm-auth-proxy` for real GitHub OAuth.

---

## 🎨 UI Color Guide

**Status Colors:**
- 🟢 Green: Success, Running, Connected
- 🟡 Yellow: Pending, Provisioning
- 🔵 Blue: In Progress (Running operations)
- 🟠 Orange: Stop action
- 🔴 Red: Failed, Delete action
- 🟣 Purple: Snapshots
- 🩵 Cyan: Activity/Operations

**Buttons:**
- Green "Start" - Launch workspace
- Orange "Stop" - Stop workspace
- Blue "Restart" - Restart workspace
- Cyan "Activity" - View operations
- Purple "Snapshots" - Manage snapshots
- Red "Delete" - Remove workspace

---

## 🐛 Troubleshooting

### Workspace stuck in "Creating"

1. Click **Activity** button
2. Check "Create" operation status
3. Common causes:
   - Docker pull timeout
   - Network issues
   - Invalid template
4. Fix: Delete and recreate

### Connection info missing

1. Operation might still be running
2. Check **Activity** for operation status
3. Wait for Success status
4. Connection info appears automatically

### Snapshot restore failed

1. Click **Activity**
2. Check "SnapshotRestore" operation
3. Common causes:
   - Snapshot file deleted from `/tmp/vm-snapshots/`
   - Corrupted tar file
   - Out of disk space
4. Error message shows specific issue

### "Open in Claude Code" doesn't work

1. Ensure Claude Code is installed
2. Check workspace is Running (green status)
3. Verify `vscode://` protocol handler is registered
4. Alternative: Use SSH command with copy button

---

## 📚 Next Steps

- **Phase 3**: Add log streaming and workspace detail pages
- **Production**: Configure real OAuth with oauth2-proxy
- **Testing**: Add integration tests and UI tests
- **Monitoring**: Set up metrics and alerting

---

## 🆘 Getting Help

**Check logs:**
```bash
# API server logs
cargo run --bin vm-api

# Docker logs
docker ps  # Find container
docker logs {container-id}
```

**Database inspection:**
```bash
sqlite3 ~/.vm/api/vm.db
> .tables
> SELECT * FROM workspaces;
> SELECT * FROM operations WHERE status = 'Failed';
```

**API endpoints:**
- Health: `http://localhost:3000/health`
- Workspaces: `http://localhost:3000/api/v1/workspaces`
- Operations: `http://localhost:3000/api/v1/operations`
