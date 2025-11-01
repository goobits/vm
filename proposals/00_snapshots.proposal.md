## Problem

Our current “examples box” workflow cannot persist developer state. Destroying a VM resets container filesystems, installed packages, volume data, and local configuration; recreating the box from `examples/` only rebuilds the base Compose stack. Database backups (`vm db backup/restore`) partially mitigate data loss but ignore the rest of the environment. As a result:

- Context switches are slow because engineers must manually replay setup steps.
- Experiments are risky; recovering from mistakes requires full rebuilds.
- Sharing a working environment with teammates is impractical.
- The legacy examples workflow competes with the proposed snapshot concept, creating duplicated code paths and documentation.

## Goals

Deliver a single, comprehensive snapshot system that replaces the “super cool box” experience everywhere. “Done” means:

1. Users can save and restore the entire VM state (containers, volumes, configs, metadata) with `vm snapshot <verb>`.
2. Snapshots work for every Docker Compose-based VM (single or multi-instance) today.
3. Provider APIs expose snapshot hooks so future providers can adopt the same UX.
4. All legacy box/example tooling, docs, and references are removed or redirected to the snapshot workflow.

## Non-Goals

- Supporting non-Compose providers (Tart/Vagrant) on day one. They will return “unsupported” but share the same CLI wiring.
- Deduplicating snapshot storage layers. Users manage disk via `vm snapshot delete`.
- Hot snapshotting running containers; the workflow stops services before capture to guarantee consistency.

## Architecture Overview

### Storage Layout

```
~/.config/vm/snapshots/<project>/<snapshot-name>/
    metadata.json
    compose/
        docker-compose.yml
        vm.yaml
    images/
        <service>.tar   # result of docker commit && docker save
    volumes/
        <volume>.tar.gz
```

### Snapshot Capture Flow

1. Discover all running Compose services (`docker compose ps --services`).
2. For each container:
   - `docker commit` → temporary image tag `vm-snapshot/<project>/<service>:<name>`
   - `docker save` the tag into `images/<service>.tar`
3. Enumerate named volumes from `docker compose config --volumes`. For each, run `docker run --rm -v <volume>:/data busybox tar czf - /data`.
4. Copy `vm.yaml`, generated `docker-compose.yml`, and any provider override files into `compose/`.
5. Record metadata:
   - Snapshot name, created_at (UTC), git commit hash + dirty flag, Docker image digests, volume sizes, user-supplied description, project identifier.
6. Stop the Compose application only if requested via `--quiesce`; otherwise warn that live snapshots may race.

### Snapshot Restore Flow

1. Validate metadata and ensure destination project directory matches the snapshot’s project slug.
2. Stop any running Compose stack for the project.
3. For each volume archive:
   - Remove existing volume (`docker volume rm`) or rename it for rollback.
   - Create a fresh volume and restore data via helper container untar.
4. Load images (`docker load < images/<service>.tar`) and retag them to the service names.
5. Overwrite `vm.yaml`/`docker-compose.yml` with archived copies (backing up current files).
6. Run `docker compose up -d` to start services using the restored state.

### CLI Surface

```
vm snapshot create <name> [--description "text"] [--project myproj]
vm snapshot list [--project myproj]
vm snapshot restore <name> [--project myproj] [--force]
vm snapshot delete <name> [--project myproj]
vm snapshot clean --max-age 30d --max-count 5
```

### Provider Integration

- Extend `Provider` trait with `snapshot(&self, SnapshotRequest)` and `restore(&self, SnapshotRestoreRequest)`.
- Implement fully for `DockerProvider`.
- Other providers return `ProviderError::Unsupported("snapshot")`.
- `SnapshotManager` orchestrates filesystem layout, metadata, and dispatches to the active provider.

### Legacy Removal

1. Delete examples-based box creation scripts/configs that overlap with snapshot functionality.
2. Update docs (`README.md`, `docs/user-guide/configuration.md`, `examples/README.md`) to reference snapshots exclusively.
3. Provide a migration guide: “If you previously duplicated examples/<box>, run `vm snapshot create <name>` instead.”
4. Remove CLI commands that spin up legacy boxes, or make them wrappers around snapshot-aware flows before final removal.

## Implementation Checklist

- [x] **Foundations**
  - [x] Create `rust/vm/src/commands/snapshot.rs` with `SnapshotManager`
  - [x] Define `SnapshotInfo` struct and JSON serialization
  - [x] Add config entry for `snapshots_dir`
- [x] **CLI Commands**
  - [x] Wire `vm snapshot <subcommand>` parsing
  - [x] Implement command handlers calling `SnapshotManager`
- [x] **Docker Provider**
  - [x] Container commit/save helpers
  - [x] Volume archive/restore helpers
  - [x] Compose config capture utilities
  - [x] Safety checks (disk space, running state)
- [x] **Metadata + UX**
  - [x] Human-readable metadata (timestamps, git hash, size)
  - [x] `vm snapshot list` formatting (name, created_at, size, description)
  - [x] Destructive-action confirmations (`restore`, `delete`)
- [x] **Legacy Deletion**
  - [x] Remove redundant example box generator scripts
  - [x] Prune docs referencing the deprecated workflow
- [x] **Verification**
  - [x] Automated tests for metadata serialization
  - [x] Integration script exercising create → delete → restore cycle
  - [x] Manual QA checklist for multi-instance projects
- [ ] **Missing Feature**
  - [ ] Implement `vm snapshot clean --max-age <duration> --max-count <num>` command

## Success Criteria

- Running `vm snapshot create <name>` captures filesystem + volume + config state within one command.
- `vm snapshot restore <name>` reproduces a working environment indistinguishable from the moment of capture.
- Snapshot list shows name, creation time, size, description, and git hash.
- All Compose-based providers use the new workflow; legacy examples/boxes are removed, and docs point solely to snapshots.
- Existing regression suite passes; new snapshot integration tests cover save/restore/migrate flows.
