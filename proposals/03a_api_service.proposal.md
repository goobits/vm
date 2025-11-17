# 03a: API Service Implementation

## Problem

The `vm` CLI is powerful but lacks a centralized, user-friendly interface for managing transient development environments. A web-based or programmatic API is needed to simplify workspace lifecycle management, especially for team-based workflows and to enable a "Claude Code web" style experience.

## Solution(s)

Develop a new, stateful RESTful API service (`vm-api`) as a separate binary within the existing Rust workspace. This service will reuse the existing `vm-*` library crates (`vm-config`, `vm-provider`, `vm-core`) to handle core logic, ensuring consistency with the CLI. The API runs on a centralized host (or small HA pair) that already manages our shared Docker services, so it becomes the single source of truth for team workspaces while individual developers can continue to use the CLI for personal/local environments. The service will manage workspace state via a local database and expose the minimum surface needed for a 100-person team to create, operate, and retire transient machines without touching the CLI. To keep the public/private boundary clean, introduce a dedicated `vm-orchestrator` crate that owns the business logic and is consumed by `vm-api` (and any future front doors such as gRPC or workers).

### Deployment Model

- `vm-api` runs on the centralized Docker host that already runs shared Postgres/Redis/auth services. That host (and its mirror for HA) is the canonical home for hosted workspaces.
- Local CLI workflows continue to manage per-developer containers on their laptops; there is no overlap or synchronization requirement between CLI-managed and API-managed environments.
- State for hosted workspaces lives in `~/.vm/api/vm.db` on the server (backed up automatically prior to migrations).
- GitHub OAuth (via `vm-auth-proxy`) fronts all HTTP traffic; the same identity provider we use for existing internal services applies here.
- Phase 1 scope is intentionally small: orchestrator CRUD + REST endpoints + list/create/delete UI. Lifecycle operations, log streaming, and shell access arrive in later phases only if usage justifies them.

## Checklists

- [ ] Scaffold a new binary crate `vm-api` within the `rust/` directory using `axum` or `actix-web`.
- [ ] Scaffold a companion `vm-orchestrator` crate that exposes business-logic APIs (`workspaces::create`, `workspaces::start`, etc.) and coordinates calls into `vm-provider`, `vm-config`, and the persistence layer.
- [ ] Add a basic health check endpoint (e.g., `/health`).
- [ ] Define an OpenAPI v3 specification (`openapi.yaml`) for the API and keep it versioned with the code; document the deployed API host and its relationship to local CLI commands (API-managed workspaces live on the shared host only).
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
- [ ] Integrate with `vm-config` and `vm-provider` crates to execute workspace creation and deletion on the centralized host (CLI-managed local environments remain independent by design).
- [ ] Add a persistence layer using SQLite and `sqlx` to store workspace metadata (default path: `~/.vm/api/vm.db` on Linux/macOS, `%APPDATA%\\vm\\api\\vm.db` on Windows).
- [ ] Add schema migrations using `sqlx::migrate!`, ship them with the repo, and provide a `vm-api migrate` command (with automatic backup of the DB before applying changes).
- [ ] Implement basic security using the `vm-auth-proxy` crate to protect all endpoints (sessions + API tokens).
- [ ] Persist lifecycle operations and TTL expirations so a janitor job (e.g., a background tokio task spawned within `vm-api`) can clean idle workspaces automatically.
- [ ] Defer optional plugin discovery until after core lifecycle parity is complete, but ensure the schema lets `POST /workspaces` include plugin IDs when available.
- [ ] Document deployment guidance (single-node Docker host, reverse proxy in front of `vm-api`, reuse existing GitHub OAuth configuration via `vm-auth-proxy`).

## Success Criteria

- The `vm-api` service compiles and runs successfully.
- All endpoints defined in the checklist are implemented and functional, including lifecycle actions, operations tracking, and snapshot management.
- API endpoints are protected and require authentication (GitHub OAuth via `vm-auth-proxy`, same flow as other internal services).
- A workspace can be created from a repo/template, started/stopped/rebuilt, and destroyed via API calls.
- Workspace events, logs, and operations are visible through the API/WebSocket interfaces.
- The service persists workspace and operation state in its SQLite database and enforces TTL-based cleanup.
- HTTP transport, orchestrator logic, and provider/database integrations remain separated so additional entry points (cron jobs, workers, other UIs) can call `vm-orchestrator` without touching HTTP code.

## Benefits

- Provides a programmatic interface for managing `vm` workspaces.
- Decouples core logic from a specific client (CLI vs. Web).
- Creates the necessary backend foundation for a web-based management UI with real-time visibility.
- Enables automation and integration with other developer tools.
- Keeps scope targeted for a ~100-person team through quotas, TTLs, and a single-node SQLite backend while leaving headroom to evolve later; staged rollout (CRUD → lifecycle → optional shell) lets us stop after each milestone if adoption goals are met.
