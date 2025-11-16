# 03a: API Service Implementation

## Problem

The `vm` CLI is powerful but lacks a centralized, user-friendly interface for managing transient development environments. A web-based or programmatic API is needed to simplify workspace lifecycle management, especially for team-based workflows and to enable a "Claude Code web" style experience.

## Solution(s)

Develop a new, stateful RESTful API service (`vm-api`) as a separate binary within the existing Rust workspace. This service will reuse the existing `vm-*` library crates (`vm-config`, `vm-provider`, `vm-core`) to handle core logic, ensuring consistency with the CLI. The service will manage workspace state via a local database and expose the minimum surface needed for a 100-person team to create, operate, and retire transient machines without touching the CLI. To keep the public/private boundary clean, introduce a dedicated `vm-orchestrator` crate that owns the business logic and is consumed by `vm-api` (and any future front doors such as gRPC or workers).

## Checklists

- [ ] Scaffold a new binary crate `vm-api` within the `rust/` directory using `axum` or `actix-web`.
- [ ] Scaffold a companion `vm-orchestrator` crate that exposes business-logic APIs (`workspaces::create`, `workspaces::start`, etc.) and coordinates calls into `vm-provider`, `vm-config`, and the persistence layer.
- [ ] Add a basic health check endpoint (e.g., `/health`).
- [ ] Define an OpenAPI v3 specification (`openapi.yaml`) for the API and keep it versioned with the code.
- [ ] Implement the initial control-plane endpoints from the spec:
    - [ ] `POST /api/v1/workspaces`: Create a workspace from repo template/config (repo URL/branch, template ID, provider, resource caps, TTL, provisioning commands, plugin selections).
    - [ ] `GET /api/v1/workspaces`: List workspaces plus status, TTL, repo metadata, labels, current operation.
    - [ ] `GET /api/v1/workspaces/{id}`: Fetch a single workspace, including connection info and active tunnels.
    - [ ] `PATCH /api/v1/workspaces/{id}`: Rename workspace, adjust TTL/labels, or toggle metadata-only settings.
    - [ ] `POST /api/v1/workspaces/{id}/actions/{start|stop|restart|rebuild}`: Lifecycle management mapped to provider operations.
    - [ ] `POST /api/v1/workspaces/{id}/commands`: Run a non-interactive command; respond with streamed logs or operation reference.
    - [ ] `DELETE /api/v1/workspaces/{id}`: Destroy a workspace and purge related volumes/snapshots.
    - [ ] `GET/POST /api/v1/workspaces/{id}/snapshots`: List and create snapshots using existing snapshot primitives.
    - [ ] `POST /api/v1/workspaces/{id}/snapshots/{snapshot_id}/restore`: Restore a workspace (or create a new one) from a saved snapshot.
    - [ ] `GET /api/v1/templates`: List workspace templates/detected presets.
    - [ ] `GET /api/v1/operations` & `GET /api/v1/operations/{id}`: Track asynchronous tasks (create, rebuild, snapshots) for the UI.
    - [ ] WebSocket channels (`/ws/workspaces/{id}/events`, `/ws/workspaces/{id}/shell`) for real-time events/log streaming.
- [ ] Integrate with `vm-config` and `vm-provider` crates to execute workspace creation and deletion.
- [ ] Add a persistence layer using SQLite and `sqlx` to store workspace metadata (default path: `~/.vm/api/vm.db` on Linux/macOS, `%APPDATA%\\vm\\api\\vm.db` on Windows).
- [ ] Implement basic security using the `vm-auth-proxy` crate to protect all endpoints (sessions + API tokens).
- [ ] Persist lifecycle operations and TTL expirations so a janitor job (e.g., a background tokio task spawned within `vm-api`) can clean idle workspaces automatically.
- [ ] Defer optional plugin discovery until after core lifecycle parity is complete, but ensure the schema lets `POST /workspaces` include plugin IDs when available.

## Success Criteria

- The `vm-api` service compiles and runs successfully.
- All endpoints defined in the checklist are implemented and functional, including lifecycle actions, operations tracking, and snapshot management.
- API endpoints are protected and require authentication.
- A workspace can be created from a repo/template, started/stopped/rebuilt, and destroyed via API calls.
- Workspace events, logs, and operations are visible through the API/WebSocket interfaces.
- The service persists workspace and operation state in its SQLite database and enforces TTL-based cleanup.
- HTTP transport, orchestrator logic, and provider/database integrations remain separated so additional entry points (cron jobs, workers, other UIs) can call `vm-orchestrator` without touching HTTP code.

## Benefits

- Provides a programmatic interface for managing `vm` workspaces.
- Decouples core logic from a specific client (CLI vs. Web).
- Creates the necessary backend foundation for a web-based management UI with real-time visibility.
- Enables automation and integration with other developer tools.
- Keeps scope targeted for a ~100-person team through quotas, TTLs, and a single-node SQLite backend while leaving headroom to evolve later.
