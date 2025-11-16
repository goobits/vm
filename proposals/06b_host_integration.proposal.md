# 06b: Host Integration Defaults

## Problem

Feedback suggested we still needed to build “host integration” basics (Git config propagation, timezone detection, sensible shell defaults). Code review shows those paths already exist and ship enabled by default:

- `host_sync.git_config` defaults to `true` (`rust/vm-config/src/config.rs:780-798`) and `AppConfig::load` copies host Git settings whenever the option is on (`rust/vm-config/src/lib.rs:74-96`).
- `vm.timezone` ships as `auto` in the embedded defaults (`configs/defaults.yaml:18-30`), and automatic detection fills in the real timezone during config load via `detector::os::detect_timezone` (`rust/vm-config/src/detector/os.rs:64-98`).
- The Dockerfile template already exports `EDITOR`, `VISUAL`, `PAGER`, and `TERM` for every container (`rust/vm-provider/src/docker/Dockerfile.j2:23-31`).

The real gap is documentation: new proposals assumed these behaviors were missing because we have not called them out explicitly in the specs or user guides. We should clarify the current defaults and highlight the toggles that are already available.

## Solution(s)

- Document the existing host integration pipeline so contributors understand what is already “on”:
  - Update proposal text and user docs to state that Git config sync, timezone detection, and shell environment defaults are active out of the box.
  - Add troubleshooting tips (e.g., how to disable `host_sync.git_config` or force a timezone).
- Add regression tests to guard these defaults (smoke test in CI that `vm init` produces a config with `host_sync.git_config: true` and `vm.timezone: auto`).

## Checklists

- [ ] Proposal & documentation updates
    - [ ] Reflect the current defaults inside this proposal and cross-link to relevant files.
    - [ ] Mention these defaults in `docs/user-guide/configuration.md` and the marketing site docs.
- [ ] Testing / validation
    - [ ] Add an integration test covering Git config syncing + timezone detection (see `rust/vm-config/src/detector/tests/host_integration_tests.rs` as a starting point).
    - [ ] Add a CLI smoke test ensuring `vm init` outputs `host_sync.git_config: true` unless explicitly disabled.

## Success Criteria

- Contributors reviewing proposal 06b immediately see that Git config copy, timezone auto-detection, and default shell env vars are already implemented and enabled.
- Documentation tells end users how to opt out or customize these host integrations.
- CI coverage prevents regressions that would silently disable these defaults.

## Benefits

- Aligns planning documents with reality, reducing duplicated work.
- Makes the “works out of the box” experience visible to the team (and future CLIs/UIs).
- Gives us tests that will alert the team if future refactors accidentally turn these features off.
