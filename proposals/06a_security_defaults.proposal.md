# 06a: Security Defaults & Secrets Handling

## Problem

Earlier discussions suggested we still needed to implement “secure by default” behaviors for built-in databases (random passwords, backups, swap defaults, bind addresses). A review of the current codebase shows most primitives already exist, but they are not consistently applied:

- **Random credentials** — `vm/src/services/mod.rs:get_or_generate_password` already provisions 16‑char secrets for shared PostgreSQL/MySQL/Redis/MongoDB containers, yet the Docker provider templates (`rust/vm-provider/src/docker/template.yml:128-218`) and the Ansible service definitions (`rust/vm-provider/src/resources/services/service_definitions.yml:1-90`) still default to `postgres/postgres`, `mysql/mysql`, etc. Project containers never read from the generated secrets.
- **Database backups** — `rust/vm/src/commands/vm_ops/destroy.rs:22-82` performs `backup_on_destroy` synchronously during `vm destroy`, blocking user flows when multiple services are enabled.
- **Swap/swapiness defaults** — `configs/defaults.yaml:18-29` already define `swap: 2048` and `swappiness: 60`, but we do not use platform heuristics (e.g., reduce defaults for macOS/Windows) nor document that these values can be overridden globally.
- **Port binding** — Defaults already restrict managed services to `127.0.0.1` (`configs/defaults.yaml:24`, `service_definitions.yml:21-63`). Changing defaults to `0.0.0.0` would be a regression; instead we need an explicit opt‑in if teams require LAN access.

## Solution(s)

1. **Secrets plumbing**
   - Wire the generated passwords into project containers by surfacing them through `vm-config` and the Docker templates (fallback to legacy values only when `host_sync.git_config` or service overrides disable secret generation).
   - Update documentation/examples (`docs/user-guide/shared-services.md`) to reference `${POSTGRES_PASSWORD}`/`${MYSQL_PASSWORD}` rather than hard-coded strings.

2. **Background backups**
   - Run `backup_on_destroy` in a detached task or queue so `vm destroy` can complete quickly while backups finish in the background (progress reported separately).

3. **Swap defaults**
   - Keep the existing defaults for Linux hosts (2 GB / swappiness 60) but document and enforce smaller values on macOS/Windows where swap files are expensive (e.g., 1 GB / swappiness 30 on macOS, 512 MB / disabled on Windows).

4. **Optional port binding setting**
   - Expose a `vm.port_binding` override (already present in schema) through CLI prompts and docs, clarifying that the default remains `127.0.0.1` for safety.

## Checklists

- [ ] Surface generated passwords inside project containers
    - [ ] Extend `vm-config` service settings to load cached secrets when none are specified.
    - [ ] Update `rust/vm-provider/src/docker/template.yml` and Ansible templates to consume those settings (drop `default(value="postgres")` fallbacks unless explicitly overridden).
    - [ ] Document how to regenerate/delete secrets.
- [ ] Make `backup_on_destroy` asynchronous
    - [ ] Move work into a tokio task or job queue triggered inside `vm destroy`.
    - [ ] Stream completion/failure to the user (e.g., via `vm-print` or notifications).
    - [ ] Ensure backups still honor `global_config.backups.keep_count`.
- [ ] Platform-aware swap defaults
    - [ ] Detect host OS and set `vm.swap`/`vm.swappiness` accordingly during `vm init`.
    - [ ] Describe the heuristic in `docs/user-guide/configuration.md`.
- [ ] Port binding opt-in
    - [ ] Keep `127.0.0.1` as the default; add CLI/docs guidance for changing it.
    - [ ] Validate that service templates respect the configured binding before exposing the toggle.

## Success Criteria

- Project containers no longer emit hard-coded database credentials; secrets flow from the existing password store into both shared services and per-project services.
- Destroy operations trigger backups without blocking the main workflow.
- Swap defaults adapt to host OS and are clearly documented.
- Binding to non-localhost remains opt-in, preventing accidental exposure while supporting teams that explicitly need it.

## Benefits

- Leverages the already-implemented primitives instead of rebuilding them.
- Reduces manual credential management for developers.
- Improves UX of `vm destroy` by making backups non-blocking.
- Documents the true default behaviors so proposals and future planning stay aligned with the codebase’s current state.
