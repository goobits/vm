# Package Infrastructure

VM manages one private package appliance for npm, Cargo, Python, and immutable
guest tools/collections. Package and collection changes use the same managed
checkout, review, integration, and release workflow inside writable guests.

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
    + workers         review, release, consumer upgrades
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
Their scoped, consumer-bound package capability can submit only assigned work.
The gateway alone joins a host-facing controller bridge; all
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

## Change And Release A Source

From a writable Docker or Tart project environment, request an isolated source:

```bash
vm packages status  # read-only connection and scoped-credential check

vm packages checkout agent-skills \
  --agent codex \
  --task "update the owner checklist"
```

Inside a managed guest, `vm packages status` verifies the workflow gateway and
consumer-bound agent credential using read-only requests. It creates no
checkout and does not repair, publish, or activate anything.

Package reconciliation installs the matching Linux `vm` guest client from the
authenticated appliance and verifies its SHA-256 digest. `vm tools update`
repairs existing environments in place; it does not require rebuilding or
recreating them. If an older appliance does not yet expose the client, run
`vm packages up` on the controller host once, then rerun `vm tools update`.

The appliance fetches the registered canonical repository, creates a unique
task branch, and returns a writable checkout under
`~/.local/share/vm/package-checkouts/<checkout-id>`. Edit that returned path,
bump the source's stable semantic version, and commit the intended changes.
Never edit an installed release under `~/.local/share/vm-tools/releases`; those
directories are read-only immutable activation output. Reconciliation replaces
older writable installations from the private artifact rather than trusting
their contents.

Finish from the same guest:

```bash
vm packages release <checkout-id>
```

That resumable command submits the exact Git bundle, waits for isolated review,
integrates against the latest canonical branch, reruns checks, and lets the
credential-isolated release worker push the commit and tag and publish the
immutable artifact. No host checkout, host approval, npmjs.org, crates.io, or
PyPI publication participates. Package and tool-collection work use this same
boundary.

For a language package, the unpublished checkout is attached only to the
assigned consumer; other consumers stay on their published versions. Every
mutating step is idempotent and receipted, so rerunning the same release resumes
after a worker restart. Successful publication removes temporary checkout data
without touching the registered repository or its persistent canonical mirror.

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

Store the canonical-source Git token as a controller secret:

```bash
vm packages auth --github
# Or import a Git token from another provider/file:
vm packages auth --token-file /secure/input/git-token
```

`--github` reads the active `gh auth token` directly into the private
controller secret without printing it. If GitHub reports an invalid session,
run `gh auth login --hostname github.com` once and retry. Input files are read
once. The token is exposed only to the source and release services that need
it. Public-only sources do not need a Git credential.

Register each package repository and each consumer inventory:

```bash
vm packages register ./packages/*
vm packages register ./packages --recursive

vm packages register auth \
  --ecosystem cargo \
  --repository https://github.com/example/auth.git

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
root `vm-tool.yaml` (`kind: binary` or `kind: collection`) is registered as a
tool source and skipped by language-package registration. This lets tool collections
such as `agent-skills` live beside npm, Cargo, and Python sources without being
misclassified by their metadata files. Registered tool collections enter the
same managed checkout workflow as language packages.

Path registration detects `package.json`, `Cargo.toml`, or `pyproject.toml`,
then reads each repository's `origin` and default branch. Every supplied path
must be a Git repository root. Use `--ecosystem` when a repository intentionally
contains more than one supported manifest. Discovery only registers metadata;
`vm packages up` does not copy, mount, build, publish, or continuously watch the
local source.
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
vm tools list
vm tools show agent-skills
```

Use `vm packages checkout <tool>` and `vm packages release <checkout-id>` to
change a registered collection. The same reviewer and release workers validate,
integrate, push, archive, and receipt its exact commit. Projects select
versions through the one-level `tools:` map in `vm.yaml`. A collection such as
`agent-skills` is one atomic version, even when it activates into several agent
directories.

The Vibe base owns Antigravity, Claude Code, and Codex executables. They do not
require this appliance; `agent-skills` remains an intentionally managed tool.
For the built-in `agent-skills` selection, `vm tools update` automatically
registers the canonical Goobits repository when it is missing. Tool updates
activate inside an already-running
environment and do not require a base rebuild. Read credentials travel to the
guest over standard input rather than command arguments. Collection activation
merges individual skills into an existing agent skill directory, preserving
unmanaged personal and system skills. Managed source releases currently support
registered collections; binary publishers remain tool-specific. A published
collection becomes eligible in each configured guest under its update policy.
Normal shell reconciliation adopts it without approval; an already-running
agent session is not hot-reloaded.

```bash
vm tools refresh
vm tools status [environment]
vm tools update [environment]
vm tools update [environment] --background
vm tools update --fleet [--provider docker] [--pattern 'project-*']
```

Omitted versions track the latest release. Explicit semantic versions remain
pinned. An explicit `update` installs every eligible configured change without a
checklist. An `off` policy disables newer-release upgrades, but not a required
first install or pinned-version repair. Normal startup never waits for the
registry, an update prompt, a guest download, or base-owned Codex repair. It
launches only cached automatic tool
work and the Vibe runtime probe/repair in the background. Prompt-policy upgrades
remain pending for an explicit `vm tools update`. The
[configuration guide](configuration.md#managed-tools-and-ai-state) owns the
`tools` policy syntax.
When no managed tools are selected, update can repair base-owned Codex without
requiring a tool catalog or package-appliance connection.

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
named volume and leave the primary environment and base image intact. Both also
repair managed client files in place so a new shell no longer depends on the
primary container's creation-time environment. `vm ssh` and `vm exec` perform
that package-edge and client-file repair before entering the guest or running
the requested command. Tool and Codex reconciliation remains shell-specific.
The explicit tool update then
invokes base-owned Codex reconciliation in the foreground, waiting for any
shell-triggered repair already in flight, before it verifies managed-tool links.
A Codex replacement is staged and validated before activation, rolls back on
failure, never writes executable content through host-synced `~/.codex`, and
does not overwrite an unmanaged `/usr/local/bin/codex` launcher.
A matching installed release with broken links is treated as non-consumable and
retried, including by the cached background startup path.

Interactive-shell reconciliation is single-flight at each ownership boundary:
one controller catalog refresh may run at a time, as may one job of each guest
reconciliation type. A successful shell-triggered pass is reused for 60
seconds, so a burst of `vm ssh` sessions does not repeat downloads or probes.
Explicit `refresh` and `update` commands bypass the recent-success window while
still respecting active locks.

`vm tools update --fleet` starts matching managed environments in place and
reconciles the shared package routing, base-owned Codex runtime, and loaded
managed-tool selection. It does not project the invoking project's application
services onto unrelated environments.

Administrative package/tool commands are host-only. Checkout, show, and release
are intentionally guest-safe. Other commands invoked inside a managed guest
print the exact shell-safe host command, for example `Run on the host: vm tools
update dev`.

## Consumer Dependency Updates

After publication, the persistent rollout worker finds every registered
consumer pinned to an older version. It clones each consumer independently,
updates its root manifest and lockfile, runs its checks, and pushes a dedicated
review branch. There is no host rollout or sync command.

Use these read-only commands to inspect progress:

```bash
vm packages consumers auth
vm packages drift
```

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
- Guest workflow reads are filtered to the capability's assigned consumer;
  shared package and tool catalogs remain readable.
- Project and integration agents never receive Git credentials.
- Project environments consume registry protocols and never mount registry
  volumes.
- Worktrees are isolated by checkout or rollout ID.
- Only credential-isolated appliance jobs can read canonical sources or publish
  private artifacts; project agents can only advance assigned checkouts.
- Receipts contain identities, commits, digests, outcomes, and timestamps—not
  secrets.

On the controller host, use `vm packages status` for runtime health and
`vm packages doctor` for the gateway, Compose definition, credentials, and
workflow service checks.
