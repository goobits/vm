# 03b: Web Interface Implementation

## Problem

Users need a visual, web-based interface to interact with the `vm` environment. A simple point-and-click UI would lower the barrier to entry and make managing development workspaces more intuitive than a command-line-only interface.

## Solution(s)

Develop a new set of UI components within the existing `site/` Svelte/Vite project. This new UI will be served under a dedicated route (e.g., `/app`) and will consume the new `vm-api` service to display and manage workspaces. The initial cut should focus on the must-have flows for a ~100-person team: listing workspaces, watching status/logs, running lifecycle actions, and onboarding new repos/templates without touching the CLI.

## Checklists

- [ ] Create a new page/route within the `site/` project for the workspace management UI.
- [ ] Develop a UI component to list workspaces retrieved from the `GET /api/v1/workspaces` endpoint (with filters for owner/status/TTL).
- [ ] Implement a "Create Workspace" drawer that calls `POST /api/v1/workspaces`, allowing repo URL/branch, template selection, resources, TTL, provisioning commands, and optional plugin toggles.
- [ ] Add lifecycle controls per workspace (start, stop, restart, rebuild) wired to `POST /api/v1/workspaces/{id}/actions/...`.
- [ ] Surface snapshot actions (list/create/restore) via `GET/POST /api/v1/workspaces/{id}/snapshots`.
- [ ] Provide inline log/event streaming using the `/ws/workspaces/{id}/events` channel plus a button to open an interactive shell via `/ws/workspaces/{id}/shell`.
- [ ] Add a "Delete" button to each workspace that calls `DELETE /api/v1/workspaces/{id}` with confirmation.
- [ ] Implement status/health indicators (running, stopped, creating, failed, expiring) with TTL countdowns.
- [ ] Create a UI element to display available templates/plugins from `GET /api/v1/templates` and (when available) `GET /api/v1/plugins`.
- [ ] Ensure the UI handles authentication via `vm-auth-proxy` (login redirect, token refresh, logout).
- [ ] Show operation history (create, rebuild, snapshot) using `GET /api/v1/operations`.

## Success Criteria

- The new UI is accessible in the browser at a dedicated route.
- Users can view/filter a list of all current `vm` workspaces and see their lifecycle/TTL status update in real time.
- Users can create a new workspace (repo/template selection, resources, TTL) using the UI.
- Users can start/stop/rebuild, tail logs, open a shell, and delete an existing workspace using the UI.
- Users can create and restore snapshots from the UI.
- The UI correctly reflects the state of the workspaces as reported by the API/WebSocket feeds.

## Benefits

- Greatly improves the usability and accessibility of the `vm` project.
- Provides a user-friendly alternative to the CLI.
- Lowers the learning curve for new users while keeping controls scoped for a modest-sized team.
- Establishes a foundation for a more feature-rich web IDE experience without overbuilding for multi-tenant scale.
