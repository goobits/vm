# Package Infrastructure

VM manages one private package appliance for npm, Cargo, Python, and immutable
guest tools/collections. Package
source work, validation, review, release, and rollout run in project
environments or ephemeral appliance containers—not as native Mac processes.

## Architecture

For Docker and Tart consumers together, use the Tart runtime:

```text
Mac
  vm CLI + Tart only
        |
        v
Dedicated Linux Tart VM (or Docker appliance)
  Docker Compose package appliance
    + gateway         one private URL
    + registry        npm + Cargo + PyPI + tool artifacts + cache/proxy
    + OCI cache       Docker Hub pull-through cache
    + work service    workflow state, Git mirrors, receipts
    + ephemeral jobs  review, release, rollout, maintenance
    + named volumes   artifacts, caches, state, receipts, worktrees
        |
        +-----------------------+
        |                       |
        v                       v
Docker project             Linux Tart project
  read-only edge             read-only edge
  persistent cache           persistent cache
  package clients            package clients
        |                       |
        +---- immutable releases / one leased checkout
```

The Mac stores controller configuration and launches Docker or Tart. It does
not clone package repositories, run package checks, build releases, or publish
artifacts. Project agents never receive Git credentials or registry storage.
The credential-free gateway alone joins a host-facing controller bridge; all
registry and workflow storage remains behind the appliance's internal network.

## Start the Appliance

Run appliance commands on the controller host. The appliance is shared and is
not scoped to the current project directory.

```bash
vm config set packages.source_roots /absolute/path/to/packages --global
vm packages up --runtime docker  # one-time Docker choice on macOS
vm packages doctor
```

The selected runtime is stored in controller state, so later starts from any
host directory are simply `vm packages up`. Every run safely reconciles the
appliance and recursively registers repositories below the configured absolute
source roots. Existing credentials and named volumes are retained. A
Docker-only setup does not need a Tart package VM.

Configured roots are resolved and scanned before the appliance is started or
updated. A missing or invalid root therefore fails without changing appliance
state. An existing but empty configured shelf is a successful no-op, which
allows first-run setup before repositories have been added.

On its first run, macOS selects Tart and prepares the versioned Linux base
automatically. Later runs reuse the stored runtime. Other platforms select
Docker. Use `--runtime docker` only when every consumer is Docker-based, or
`--runtime tart` to switch explicitly. The Docker gateway is loopback-bound and
is deliberately rejected for Tart consumers. The Tart appliance exposes its
gateway on the private VM address so both providers can reach it.
Explicit appliance image overrides are also reused by later runs of the same
controller version; upgrading the CLI selects that release's matching images.
When a source-installed CLI cannot pull unreleased matching images, it discovers
its checkout and builds those infrastructure images inside Docker automatically.
Local source images re-enter Docker's content-addressed build cache on each
appliance start, so service- or job-only edits cannot be hidden behind a stale
image. They carry a stable source-build marker instead of the changing controller
binary hash, so an unrelated CLI rebuild does not by itself change the effective
service image identity or force Compose recreation. Released installs remain
pull-only and never depend on a source tree.
Before the non-root registry and workflow services start, a networkless init
step repairs only their named-volume roots to the package-service UID/GID. This
keeps both fresh volumes and volumes added during an upgrade writable without
granting the long-running services root access.

VM injects the gateway and a read-only token through npm, Cargo, and pip
environment settings whenever it creates or starts a project environment. It
also exports `VM_OCI_MIRROR`; Linux Tart guests with managed Docker activate
that mirror in Docker Engine automatically.
Projects keep ordinary versioned dependencies; no local/remote branch belongs
in application code.

Each worker gets a small read-only package edge. Docker runs it as a Compose
sidecar; Linux Tart runs the same image in the guest's Docker Engine. Package
clients talk only to that stable local endpoint. The edge keeps a persistent,
last-known-good internal catalog and separate internal/public caches, while the
central appliance remains authoritative for new internal artifacts and package
work.

If the central appliance is temporarily unavailable, installed dependencies
keep working, cached locked internal artifacts remain available, and packages
classified as external can still reach their public upstream. An uncached
internal package fails clearly and never falls back to a similarly named public
package. Before the edge has obtained its first catalog snapshot, all cache
misses fail closed. If the local edge itself is stopped, new package-manager
requests fail until the project stack restarts it; the sidecar uses
`restart: unless-stopped` and does not hold up an interactive shell with update
checks.

The OCI cache shares the private gateway's `/v2/` route but has its own named
volume. It accepts pulls only; Distribution proxy mode rejects pushes. VM never
rewrites the Mac Docker daemon. Docker-hosted projects may opt their external
daemon into the exported mirror, while Tart's nested Linux daemon is configured
inside the guest.

## Register Sources and Consumers

Store Git and optional remote-registry tokens as controller secrets:

```bash
vm packages auth --github
# Or import a Git token from another provider/file:
vm packages auth --token-file /secure/input/git-token
vm packages auth --ci-token-file /secure/input/registry-token
```

`--github` reads the active `gh auth token` directly into the private
controller secret without printing it. If GitHub reports an invalid session,
run `gh auth login --hostname github.com` once and retry. Input files are read
once. The tokens are exposed only to the scoped service or release job that
needs them. Public-only sources do not need a Git credential.

Register each package repository and each consumer inventory:

```bash
vm packages register ./packages/*
vm packages register ./packages --recursive

vm packages register auth \
  --ecosystem cargo \
  --repository https://github.com/example/auth.git \
  --ci-registry https://ci-registry.example/cargo/index/

vm packages consumer register project-a \
  --repository https://github.com/example/project-a.git \
  --dependency auth@1.4.2
```

The registration path may be an absolute host directory and appliance commands
may be run from any host directory. Configure any number of controller-wide
source roots when they should be reconciled on every `vm packages up`:

```bash
vm config set packages.source_roots \
  /absolute/path/to/packages \
  /another/absolute/source-root \
  --global
vm packages up
```

Keep that source shelf flat and make each child an independent Git repository:

```text
/absolute/path/to/packages/
├── agent-skills/
├── auth/
└── shared-config/
```

The shelf itself should not be a Git repository. A repository with a valid
root `vm-tool.yaml` (`kind: binary` or `kind: collection`) is reported as a tool
source and skipped by language-package registration. This lets tool collections
such as `agent-skills` live beside npm, Cargo, and Python sources without being
misclassified by their metadata files. `vm tools` remains responsible for
publishing and activating those repositories.

Path registration detects `package.json`, `Cargo.toml`, or `pyproject.toml`,
then reads each repository's `origin` and default branch. Every supplied path
must be a Git repository root. Use `--ecosystem` when a repository intentionally
contains more than one supported manifest. Discovery only registers metadata;
it does not copy, mount, build, publish, or continuously watch the local source.
The appliance clones the registered Git origins into its private `source-mirrors`
Docker volume when package work requires them.

The host path is intentionally not stored in project configuration or mounted
into the appliance. Configured shelves may start empty; the next `vm packages
up` discovers repositories after they are added. Manual `vm packages register
<path> --recursive` remains strict and reports an error when it finds no Git
repositories. Registration is idempotent.

Supported ecosystems are `npm`, `cargo`, and `python`. A package has one
canonical repository and immutable published versions.

## Register And Consume Tools

Tool definitions use the same private appliance but remain separate from
language-package protocols:

```bash
vm tools register agent-skills \
  --kind collection \
  --repository https://github.com/example/agent-skills.git
vm tools publish agent-skills
vm tools list
vm tools show agent-skills
```

`publish` launches a trusted ephemeral collection job. It clones the registered
branch, reads the stable semantic version from `package.json`, archives the
exact commit deterministically, and records an immutable artifact and receipt.
The remote must already contain that branch; the job never creates or pushes
source history. Project agents cannot publish it. Projects select
versions through the one-level `tools:` map in `vm.yaml`. A collection such as
`agent-skills` is one atomic version, even when it activates into several agent
directories.

The Vibe base owns Antigravity, Claude Code, and Codex executables. They do not
require this appliance; `agent-skills` remains an intentionally managed tool.
For the built-in `agent-skills` selection, `vm tools update` automatically
registers the canonical Goobits repository when it is missing. Publication is
always explicit: a fresh controller stops with the exact
`vm tools publish agent-skills` command, after which the operator reruns
`vm tools update`. Worker-edge and base-runtime reconciliation happen before
that publication check, so an unpublished collection does not prevent targeted
repair. Tool updates activate inside an already-running
environment and do not require a base rebuild. Read credentials travel to the
guest over standard input rather than command arguments. Collection activation
merges individual skills into an existing agent skill directory, preserving
unmanaged personal and system skills. The generic publisher currently supports
registered collections; binary publishers remain tool-specific.

```bash
vm tools refresh
vm tools status [environment]
vm tools update [environment]
vm tools update [environment] --all --background
```

Omitted versions track the latest release. Explicit semantic versions remain
pinned. Normal startup never waits for the registry, an update prompt, a guest
download, or base-owned Codex repair. It launches only cached automatic tool
work and the Vibe runtime probe/repair in the background. Prompt-policy upgrades
remain pending for an explicit `vm tools update`; full Codex ownership, locking,
and repair behavior is documented under [Managed Tools And AI State](configuration.md#managed-tools-and-ai-state).

`vm packages list` reports registered and published package state; installation
is environment-specific, and a published package is consumable through the
gateway. `vm tools list` reports controller registration/publication only.
`vm tools status [environment]` adds installed and consumable guest state and
reports the base-owned Codex runtime separately. Its rows are the union of
configured tools, controller registrations, and guest state, so a stale
installed tool remains visible after it is removed from project configuration.
For collections, `PROJECT_COPY` also identifies a standalone project checkout
at a declared activation path. Managed releases live under the guest home and
never advance, remove, or otherwise rewrite project Git; the operator must pick
one owner for overlapping collection content.

`vm tools update [environment]` is also the idempotent upgrade reconciliation
entry point. For Docker it regenerates current Compose metadata and updates only
a missing or stale `package-edge` sidecar with `--no-deps`. For Linux Tart it
reconciles only the guest edge container. Both paths preserve the edge cache
named volume and leave the primary environment and base image intact. It then
invokes base-owned Codex reconciliation in the foreground, waiting for any
shell-triggered repair already in flight, before it verifies managed-tool links.
A matching installed release with broken links is treated as non-consumable and
retried, including by the cached background startup path.

Interactive-shell reconciliation is single-flight at each ownership boundary:
one controller catalog refresh may run at a time, as may one job of each guest
reconciliation type. A successful shell-triggered pass is reused for 60
seconds, so a burst of `vm ssh` sessions does not repeat downloads or probes.
Explicit `refresh` and `update` commands bypass the recent-success window while
still respecting active locks.

Package and tool controller commands are host-only. When invoked inside a
managed guest, the CLI exits without changing state and prints the exact
shell-safe command to run on the host, for example
`Run on the host: vm tools update dev --all`.

## Develop and Release a Package

Run checkout from the selected consumer project:

```bash
vm packages checkout auth \
  --agent agent-17 \
  --task "fix token refresh"
```

The work service records the canonical base commit, creates a unique branch,
and returns a bundle to an isolated checkout under
`~/.local/share/vm/package-checkouts/<checkout-id>` in the project environment.
Only that project receives the unpublished override. Other consumers remain on
their published versions.

Commit the intended package changes inside the project environment, then run:

```bash
vm packages submit <checkout-id>
vm packages integrate <submission-id>
vm packages publish <submission-id> --push-source
```

Submission validates the exact bundle and selected consumers, then launches a
credential-free ephemeral reviewer. Integration is serialized against the
latest canonical commit. After integrated checks pass, the appliance removes
the mutable agent and integration worktrees and retains only the immutable
integration bundle required for release. Publication requires explicit
source-push authority, verifies the semantic version bump, pushes the matching
commit and tag, and publishes the same immutable artifact locally and to the
configured CI registry. A successful release removes that remaining bundle and
the guest's temporary checkout.

Cleanup is restricted to validated task data under `/data/agents/<checkout-id>`
inside the appliance and
`~/.local/share/vm/package-checkouts/<checkout-id>` inside the project
environment. Successful integration removes agent and integration worktrees;
successful publication removes the final immutable release bundle after its
receipt is durable. Cleanup never removes the registered source repository,
its `.git` data, `/workspace`, or the persistent canonical mirror under
`/data/sources`.

Every mutating step is idempotent and writes a durable receipt. A retry resumes
from the recorded state instead of creating a second merge, tag, or release.
Use `vm packages cancel <checkout-id>` to stop eligible work. Use
`vm packages cleanup <checkout-id>` to remove temporary data for a failed,
rejected, cancelled, or already-published checkout.

## Drift and Explicit Rollouts

Publishing never upgrades consumers automatically:

```bash
vm packages consumers auth
vm packages drift
vm packages rollout auth@1.5.0 --to project-a
```

A rollout clones the consumer inside an ephemeral job, updates only its root
manifest and lockfile, runs its checks, and pushes a dedicated review branch.
The registered consumer version changes only after its normal review process
updates the inventory. Rerun `vm packages consumer register` with the reviewed
version to refresh that inventory and close the matching rollout receipt.

## Backup and Recovery

Backups stay inside a private appliance named volume:

```bash
vm packages backup
vm packages backups
vm packages restore <backup-id>
```

Backup and restore pause the registry, OCI cache, and work services, archive
every data volume separately, and verify SHA-256 manifests before restore.
Restores are retryable after interruption. `vm packages down` preserves every
volume.

These are local operational backups; export the Docker or Tart storage through
your infrastructure backup system to protect against physical disk loss.

## Security Boundaries

- The gateway is private by default and all workflow routes are authenticated.
- Read, controller, reviewer, rollout, release, and publish credentials are
  separate.
- Project and integration agents never receive Git credentials.
- Project environments consume registry protocols and never mount registry
  volumes.
- Worktrees are isolated by checkout or rollout ID.
- Only the release job can push source/tags and publish artifacts, and the CLI
  must explicitly authorize that job.
- Receipts contain identities, commits, digests, outcomes, and timestamps—not
  secrets.

Use `vm packages status` for runtime health and `vm packages doctor` for the
gateway, Compose definition, credentials, and workflow service checks.

Run `vm tools update [environment]` to add or refresh a missing/stale worker
edge and reconcile Codex/managed tools. The project image and base do not need
rebuilding, unrelated services are not recreated, and edge cache volumes are
preserved. Stable source-image identity and background shell reconciliation are
implemented but awaiting host acceptance.
