# Security and changelog follow-up

Scope: newly generated dependency PRs #90–101 after #89 merged, concise changelog
maintenance directions, and accurate Unreleased notes. Package dependency changes
and acceptance remain tracked here; existing host environments are preserved.

- [x] Review and integrate compatible tar security, TOML, Buildx, and queued Cargo updates.
- [x] Decline unsupported Ubuntu 22.10 and prevent distro-series updates.
- [x] Document concise user-facing changelog policy; remove Unreleased noise and stale CLI syntax.
- [x] Validate dependency changes with workspace checks and hosted security/package acceptance.
- [x] Add live README badges for CI, security scanning, and coverage workflows.
- [x] Review all 33 main-branch CodeQL alerts and reproduce genuine regressions.
- [x] Fix PyPI HTML injection, auth transport boundaries, and scoped-consumer receipt persistence; validate regressions.
- [ ] Preserve per-alert evidence and apply justified dispositions; verify main-branch security results after integration.
- [ ] Close superseded PRs, merge, preserve evidence, and clean task resources/branches.

- [ ] Make all workflows opt-in and document explicit validation/publication commands.
- [ ] Inspect and repair build regressions in newly merged dependency PRs #103–108; rerun explicitly selected checks.
