# Changelog

## [Unreleased]

### Breaking changes

- The v6 CLI uses explicit, project-scoped environments: `vm create` provisions a stopped environment, `vm start` starts it, and `vm shell` / `vm exec` require it to be running. Use `--project` to select another project.
- Snapshots live under `vm snapshots`; fleet operations use `--all-envs` with optional provider and name filters. Retired commands, `vm.box`, legacy environment discovery, and platform-less snapshot archives are no longer supported.
- `vm tools update` arguments select tools; repeated `--to` options select environments. Bare `vm packages release` uses the current managed checkout or registered canonical workspace.

### Added

- Portable snapshots for Docker, Podman, and Tart, with gzip or uncompressed archives, checksum verification, explicit overwrite, and project ownership checks. Tart capture and restore require a stopped VM.
- Private package releases support background execution, receipt-based resume, consumer review, and automatic tool activation. Interrupted work can resume; stopped environments receive updates when started.
- `vm tools enable` enrolls tools across running Docker environments and future environments; update and disable commands manage the same persistent enrollment. Agent skills are distributed as managed collections.
- Fleet commands provide project-scoped targeting and structured execution/log output.
- Scoped persistent volumes and tool caches survive environment recreation. Linux Tart guests use Docker Engine; macOS guests retain an explicit Colima fallback.

### Fixed

- Environment removal preserves snapshots and persistent data unless deletion is explicit, completes configured database backups first, and deletes only exclusively owned storage.
- Failed snapshot replacement preserves the previous snapshot; restored runtimes retain their original configuration identity so later starts detect drift. Volume archives remain private and owned by the invoking user on native Linux.
- Package retries retain durable work and immutable publication state, report partial activation failures accurately, and preserve environment and volume identities during tool updates.
- Scoped consumer names persist correctly across package-service restarts.
- Source installs keep the executable independent of temporary build caches, and private package infrastructure can build matching images from the installed source checkout.
- Windows paths, profile isolation, storage ownership, and lock handling now work consistently across CLI and package operations. Native Linux package services can access their isolated Compose credentials.
- Shell bootstrap skips unchanged dependency work, and managed AI tools update in place without rebuilding environments or overwriting unrelated launchers.

### Security

- Python package indexes escape uploaded filenames and prevent executable HTML; encoded download links preserve artifact names.
- Secret clients require HTTPS for remote endpoints, refuse redirects, and keep response bodies out of errors. The built-in HTTP server binds only to loopback.
- Snapshot and package archives reject unsafe paths and links; protected host mount checks cover Unix paths, Windows drive roots, and network shares.
- Package builds isolate source commands from publication and Git credentials. Guests receive repository-bound capabilities, while authentication rejects oversized requests and compares tokens in constant time.
- Source installation requires a verified Rust installer checksum; configuration previews redact secret values.
- Require `tar` 0.4.46 or newer to fix archive-header handling that could conceal files during extraction ([advisory](https://github.com/advisories/GHSA-3pv8-6f4r-ffg2)).

## 5.0.0

### 🔧 Changed

- 🪟 Humane v5 commands center daily work on `run`, `list`, `shell`, `exec`, `logs`, `copy`, lifecycle, state, configuration, plugin, and system operations.
- 🚀 Lower-level registry and base-image workflows live under `vm system`.
- 📦 Database, fleet, and secret workflows remain plugin-backed top-level commands.
- 📚 Documentation describes the v5 command model.

### 🐛 Fixed

- ☁️ Environment removal preserves explicitly saved snapshots.
