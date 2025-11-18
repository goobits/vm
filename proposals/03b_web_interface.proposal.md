# 03b: Web Interface Implementation

**Status: ✅ Phase 1 Complete (~80% implemented)**
**Last Updated: 2025-11-18**

## Implementation Status

### ✅ What's Working Now
- `/app` route with full workspace management UI
- List workspaces with real-time status badges (creating, running, stopped, failed)
- Create workspace drawer with template selection and TTL configuration
- Full lifecycle controls (start, stop, restart, delete with confirmation)
- Snapshot management (list, create, restore)
- Operations history viewer
- Manual refresh (auto-refresh deferred to Phase 2)
- Authentication ready (header-based, compatible with vm-auth-proxy)

### ❌ Not Yet Implemented
- Auto-refresh / real-time updates
- Log streaming viewer
- Interactive shell
- Templates/plugins dynamic loading from API
- Primary navigation link in site header
- Shared/team workspace visibility

---

## How to Use Right Now

### Running the Web UI

```bash
# 1. Start the API service first
cd rust
cargo run --bin vm-api
# API runs on port 3121

# 2. In a separate terminal, start the web UI
cd site
npm install
npm run dev
# UI runs on port 3120, proxies /api requests to port 3121

# UI available at: http://localhost:3120/app
# Both ports (3120-3121) are in vm.yaml exposed range (3120-3129)
```

### Using the Interface

1. **Navigate to `/app`**
   - Open http://localhost:3120/app in your browser
   - For local dev, you're automatically authenticated

2. **Create a Workspace**
   - Click "+ New Workspace" button
   - Enter workspace name
   - Select template (nodejs, python, rust, go)
   - Set TTL (Time To Live) in hours
   - Click "Create"

3. **Manage Workspaces**
   - **Start/Stop/Restart**: Click buttons in the Actions column
   - **Delete**: Click delete button (requires confirmation)
   - **View Details**: Expand rows to see connection info

4. **Snapshots**
   - Click "Snapshots" button on any workspace
   - Create snapshot with a name
   - Restore from previous snapshots

5. **Operations History**
   - Click "Operations" button to see workspace history
   - View create, snapshot, and other operation logs

### Connecting to Auth Proxy (Production)

```bash
# In production, deploy vm-auth-proxy in front of the site
# Auth proxy should proxy to the Svelte app and set headers:
# - X-VM-User: username
# - X-VM-Email: user@example.com

# The UI will automatically forward these headers to the API
```

---

## Problem

Users need a visual, web-based interface to interact with the `vm` environment. A simple point-and-click UI would lower the barrier to entry and make managing development workspaces more intuitive than a command-line-only interface.

## Solution(s)

Develop a new set of UI components within the existing `site/` Svelte/Vite project. This new UI will be served under a dedicated route (e.g., `/app`) and will consume the new `vm-api` service to display and manage workspaces. The initial cut should focus on the must-have flows for a ~100-person team: listing workspaces, watching status/logs, running lifecycle actions, and onboarding new repos/templates without touching the CLI.

## Checklists

- [x] Create a new page/route within the `site/` project for the workspace management UI.
- [x] Develop a UI component to list workspaces retrieved from the `GET /api/v1/workspaces` endpoint (with filters for owner/status/TTL).
- [x] Implement a "Create Workspace" drawer that calls `POST /api/v1/workspaces`, allowing template selection, TTL. (Note: repo URL/branch, provisioning commands, and plugin toggles deferred)
- [x] Add lifecycle controls per workspace (start, stop, restart) wired to `POST /api/v1/workspaces/{id}/{action}`. (Note: rebuild not yet available in API)
- [x] Surface snapshot actions (list/create/restore) via `GET/POST /api/v1/workspaces/{id}/snapshots`.
- [ ] Provide inline log/event streaming using the `/ws/workspaces/{id}/events` channel plus a button to open an interactive shell via `/ws/workspaces/{id}/shell`. (WebSocket endpoints not yet implemented)
- [x] Add a "Delete" button to each workspace that calls `DELETE /api/v1/workspaces/{id}` with confirmation.
- [x] Implement status/health indicators (running, stopped, creating, failed, expiring) with TTL countdowns.
- [ ] Create a UI element to display available templates/plugins from `GET /api/v1/templates` and (when available) `GET /api/v1/plugins`. (Using hardcoded templates for now)
- [x] Ensure the UI handles authentication via `vm-auth-proxy` (headers-based auth implemented, ready for proxy integration).
- [x] Show operation history (create, rebuild, snapshot) using `GET /api/v1/operations`.
- [ ] Add a primary navigation link ("Workspaces") in the site header that routes to `/app`.

## Phase 1 Acceptance Criteria - ✅ COMPLETE

**Must have (ship in 4-6 weeks):**
- [x] `/app` route loads behind auth and lists all workspaces owned by the logged-in user.
- [x] Manual refresh button (auto-refresh every 10 seconds deferred to Phase 2).
- [x] "Create Workspace" drawer with template selection + TTL; successful creates append to the list.
- [x] Delete action with confirmation (removes workspace via API and updates the table).
- [x] Status badges for `creating`, `running`, `stopped`, `failed` plus creation timestamp display.

**Bonus Items Completed Beyond Phase 1:**
- [x] Lifecycle controls (start, stop, restart) - originally Phase 2
- [x] Snapshot list/create/restore - originally Phase 2
- [x] Operations history viewer - originally Phase 2

**Explicitly deferred to later phases:**
- Interactive shell (Phase 3).
- Auto-refresh / real-time updates (Phase 2).
- Log streaming viewer (Phase 2).
- Shared/team visibility (Phase 2+) once RBAC is defined.
- Templates endpoint integration (Phase 2).

## Success Criteria

### Phase 1 (Current) - ✅ COMPLETE
- [x] The new UI is accessible in the browser at `/app` route.
- [x] Users can view a list of all current `vm` workspaces with status badges.
- [x] Users can create a new workspace (template selection, TTL) using the UI.
- [x] Users can start/stop/restart and delete existing workspaces using the UI.
- [x] Users can create and restore snapshots from the UI.
- [x] The UI correctly reflects the state of workspaces as reported by the API.
- [x] Users can view operations history for their workspaces.

### Phase 2 (Future)
- [ ] Real-time auto-refresh of workspace status (every 10 seconds)
- [ ] Users can view live logs/events via WebSocket streaming
- [ ] Rebuild action available in lifecycle controls
- [ ] Templates loaded dynamically from API
- [ ] Primary navigation link added to site header

### Phase 3 (Future)
- [ ] Interactive shell access via WebSocket
- [ ] Advanced filtering and search
- [ ] Shared/team workspace visibility with RBAC

## Benefits

- Greatly improves the usability and accessibility of the `vm` project.
- Provides a user-friendly alternative to the CLI.
- Lowers the learning curve for new users while keeping controls scoped for a modest-sized team.
- Establishes a foundation for a more feature-rich web IDE experience without overbuilding for multi-tenant scale.
