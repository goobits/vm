# Package Infrastructure

VM manages one private package appliance for npm, Cargo, Python, and immutable
guest tools. Packages, binary tools, and collections use the same durable
review, integration, and release services. Editable isolated checkouts live
only in the managed guest that requested them.

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
    + workers         review, credential-free builds, release, consumer upgrades
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

## Initialize Package Work

Run this once on the controller host:

```bash
vm packages init ~/projects/packages
```

`init` records the controller source shelf, imports an active GitHub CLI
credential when available, starts the shared appliance, and reconciles
discovered sources. It does not select a project or an agent. Existing
credentials and named volumes are retained.

`vm packages up` remains an advanced lifecycle command. It recursively
registers repositories below configured source roots and quarantines a clean,
committed unhealthy child repository under that shelf's `.vm-quarantine`
directory instead of failing unrelated sources. A dirty or unborn repository
is left untouched and reported as unresolved. The command exits successfully
and reports `degraded` when quarantine or registration failures remain. Exact
canonical sources are reconciled separately and are never quarantined or
repaired.
`vm packages doctor --fix` applies safe deterministic repairs only to managed
shelves and reports manual instructions for an unhealthy canonical source.

Configured roots are resolved and scanned before the appliance is started or
updated. A missing root therefore fails without changing appliance state. An
existing empty shelf is a successful no-op.

The appliance runs once on the controller host through Docker or Podman. The
first setup follows the configured container provider and later runs reuse the
stored engine; `vm packages up --engine docker|podman` overrides it. Docker,
Podman, and Linux Tart project environments all reach this authenticated
control plane, so Tart does not carry a second package-appliance lifecycle.
Explicit appliance image overrides are also reused by later runs of the same
controller version; upgrading the CLI selects that release's matching images.
When a source-installed CLI cannot pull unreleased matching images, it discovers
its checkout and builds those infrastructure images with the selected engine.
Local source images carry separate content fingerprints for server and job
inputs. `vm packages up` skips an unchanged image entirely, and a workflow-only
edit does not rebuild the credential-separated job image. Changed local images
use the optimized source-install Cargo profile and the engine's shared build
cache. An unrelated CLI rebuild therefore does not change image identity or
force Compose recreation. Released installs remain pull-only and never depend
on a source tree.
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

## Daily Source Work

From the managed guest where the agent is already running, create or resume the
source checkout:

```bash
vm packages checkout agent-skills
# Continue in the absolute Source path printed by the command.
```

The command derives its actor and consumer from the guest's signed capability,
prints one `Source: <absolute-path>` result, and never launches Codex, Claude,
Antigravity, or another agent. Run it again to resume the same active source.
If durable state survives but the guest copy does not, VM restores the checkout
at the same managed path without replacing local modifications.

Inside a managed guest, `vm packages status` verifies the workflow gateway and
consumer-bound agent credential using read-only requests. It creates no
checkout and does not repair, publish, or activate anything.

Package reconciliation installs the matching Linux `vm` guest client from the
authenticated appliance and verifies its SHA-256 digest. `vm tools update`
repairs existing environments in place; it does not require rebuilding or
recreating them. If an older appliance does not yet expose the client, run
`vm packages up` on the controller host once, then rerun `vm tools update`.
The same reconciliation activates installed Node and Cargo toolchains for
non-interactive package checks and refreshes the host Git author identity when
`host_sync.git_config` is enabled.

The appliance serves an immutable source bundle. The guest creates the writable
checkout under
`~/.local/share/vm/package-checkouts/<checkout-id>/source`. The agent edits
there, bumps the source's stable semantic version when required, tests, and
commits the intended changes. The reviewer consumes a durable bundle rather
than a shared checkout-volume mount.
Never edit an installed release under `~/.local/share/vm-tools/releases`; those
directories are read-only immutable activation output. Reconciliation replaces
older writable installations from the private artifact rather than trusting
their contents.

Inside that checkout, the agent finishes with:

```bash
vm packages release
```

The checkout ID is inferred from the current directory. `vm packages cancel`
uses the same inference. Controller-side ID lookup remains a hidden diagnostic,
not part of normal work.

For a language package the workflow records whether the requesting project
actually consumes it. Source-only maintenance runs package checks without
inventing a consumer result or changing the project's dependency setup. When a
project does consume the package, its checkout keeps the development override
and consumer checks for the life of that checkout.

Cancellation first restores any development override and removes the guest
copy, then closes the durable checkout. If local restoration fails, the
checkout remains cancelled rather than closed; correct the reported local
problem and rerun `vm packages cancel` from the checkout source.

That resumable command submits the exact Git bundle, waits for isolated review,
integrates against the latest canonical source, reruns checks, and lets
credential-isolated jobs publish the immutable artifact. Binary build commands
run first in a separate no-egress builder that has no Git, release, or publish
credential; the release worker consumes only its durable digest-addressed
archives. No host checkout, host approval, npmjs.org, crates.io, or PyPI
publication participates.

After a configured tool is published, normal managed-tool reconciliation or an
explicit `vm tools update <tool>` activates it in running managed Docker
environments that configure it. Repeat `--to <environment>` to limit activation. Language packages
are not installed directly; their existing consumer rollout workers remain
authoritative.

For a language package, the unpublished checkout is attached only to the
assigned consumer; other consumers stay on their published versions. Every
mutating step is idempotent and receipted, so rerunning the same release resumes
after a worker restart. If a durable active checkout outlives its scoped lease,
the same release command securely reacquires the lease from the assigned guest
and continues. A permanent release preflight failure, such as an undersized
semantic-version bump, returns the checkout with actionable feedback; edit and
commit it, then rerun the same command. The appliance restores its compacted
import target automatically before accepting the revised bundle. Validation,
review, and integration are scoped to that submitted commit, so the same
checkout can make multiple receipted rework passes without replaying stale
results. Successful publication removes temporary checkout data without
touching the registered repository or its persistent canonical mirror.

## Release From A Canonical Workspace

Use `vm packages checkout <source>` when an agent needs an isolated shared-source
checkout. When an agent already owns a registered repository mounted as its
ordinary workspace, release the committed workspace directly:

```bash
cd /workspace
vm packages release
```

Register the exact host repository once before creating or reconciling its
environment:

```bash
vm packages register ~/projects/typemill
```

The environment's physical project root must exactly match that remembered
canonical source and its origin and package or tool identity must match the
catalog. A different clone with the same origin receives no release authority.
The command requires a
clean, committed worktree, creates the durable checkout and submission
internally, and resumes the same transaction when repeated. Its lease and
resume state live under the guest's private VM state, not in the repository.
VM never cleans, resets, tags, or otherwise edits the workspace.

Canonical-workspace releases retain an immutable source bundle and digest in
the appliance. They do not push the source commit or tag, invoke GitHub
publication, or publish to a public language registry. Review, rework,
integration, private publication, and managed-tool activation continue through
the existing services.

The first internal release reviews the complete committed tree and accepts its
declared stable version. Every later release compares the complete change set
since the last successfully published internal source commit, even when several
local commits were made. It never guesses a baseline from `HEAD^` or a remote
branch.

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
it. GitHub SSH origins remain the canonical repository identity, but those
isolated services rewrite the transport to HTTPS so the token works without
receiving a host SSH key. The first checkout may need to build a full canonical
mirror; its request remains active for up to one hour instead of inheriting the
short control-plane timeout. Interrupted clones are killed and their temporary
directories are removed on retry. Public-only sources do not need a Git
credential.
Re-registration treats the HTTPS and SSH forms of the same GitHub repository
as one source, so changing a local clone's transport does not degrade catalog
reconciliation.

For source-built Docker appliances, guest package-edge identity follows the
immutable image digest rather than only the version tag. Rebuilding the same VM
version therefore updates the managed guest client without recreating the main
environment container.
Explicit cross-project environment names resolve their owning `vm.yaml` from
managed Docker metadata before reconciliation, so running `vm tools update
--to projects-dev` from another repository updates `projects-package-edge`, not
the caller's package edge. Legacy Docker Desktop bind paths are translated back
to native host paths without recreating the container.

Register exact read-only project workspaces, managed-shelf repositories, and
consumer inventory:

```bash
vm packages register ./packages/*
vm packages register ./packages --recursive
vm packages register ~/projects/typemill

vm packages register auth \
  --ecosystem cargo \
  --repository https://github.com/example/auth.git

vm packages consumer register project-a \
  --repository https://github.com/example/project-a.git \
  --dependency auth@1.4.2
```

Successful local registration remembers each physical Git root in
`packages.canonical_sources`. Appliance commands may be run from any host
directory. URL-only registration records catalog metadata but grants no
workspace-release authority. Configure controller-wide managed shelves
separately when their children should be discovered on every `vm packages up`:

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
tool source and skipped by language-package registration. This lets managed
tools live beside npm, Cargo, and Python sources without being misclassified by
their metadata files. Both tool kinds can use the managed checkout or
canonical-workspace release path.

Path registration detects `package.json`, `Cargo.toml`, or `pyproject.toml`,
then reads each repository's `origin` and default branch. Every supplied path
must be a Git repository root. Use `--ecosystem` when a repository intentionally
contains more than one supported manifest. Discovery only registers metadata;
`vm packages up` does not copy, mount, build, publish, or continuously watch the
local source.
The appliance clones the registered Git origins into its private `source-mirrors`
Docker volume when package work requires them.

Host paths are stored only in controller-global configuration and are never
mounted into the appliance. Managed shelves and exact canonical roots have
separate reconciliation policies. Configured shelves may start empty;
the next `vm packages up` discovers repositories after they are added. Manual
`vm packages register <path> --recursive` remains strict and reports an error
when it finds no Git repositories. Registration is idempotent.

Supported ecosystems are `npm`, `cargo`, and `python`. A package has one
canonical repository and immutable published versions.

## Register And Consume Tools

Tool definitions use the same private appliance but remain separate from
language-package protocols:

```bash
vm tools register agent-skills \
  --kind collection \
  --repository https://github.com/example/agent-skills.git
```

Use `vm packages checkout <tool>` inside a managed guest for isolated tool work.
From a registered canonical workspace, use bare `vm packages release`. The same
review, build, and release workflow validates, integrates, archives, and
receipts the exact commit. Managed checkouts push their integrated source;
canonical workspace releases retain the immutable source internally. Projects
select versions through the one-level `tools:` map in `vm.yaml`. A collection
such as `agent-skills` is one atomic version, even when it activates into
several agent directories.

Binary tools use a versioned, argument-safe manifest. A credential-separated
builder builds each declared target from the submitted source bundle, validates
the archive and executable links, and stages immutable content-addressed bytes.
Its package managers can resolve registry-backed npm, Cargo, and Python
dependencies through a private read-only edge. The edge holds the private
upstream read credential; repository commands receive only unauthenticated edge
URLs and no credential value. Git dependencies are intentionally unavailable in
the no-egress builder; publish them to the private registry or replace them with
registry releases before registering the binary tool.
The release worker cannot run the repository build command; it revalidates and
publishes those exact staged bytes:

```yaml
schema: 1
kind: binary
version: 1.0.0
builds:
  - target: linux-arm64
    command: ["npm", "run", "build:linux-arm64"]
    archive: dist/tool-linux-arm64.tar.gz
    links:
      .local/bin/tool: bin/tool
    verify: ["bin/tool", "--version"]
```

Build commands are argument arrays, paths must remain inside the isolated build
directory, and linked binaries must be nonempty executables. The Linux builder
currently accepts `linux-amd64` and `linux-arm64`; it does not claim macOS
artifacts it cannot verify. A deterministic command, archive, or verification
failure returns the same workspace release to `NeedsChanges`; infrastructure
failures remain retryable.

The Vibe base owns Antigravity, Claude Code, and Codex executables. They do not
require this appliance; `agent-skills` remains an intentionally managed tool.
For the built-in `agent-skills` selection, `vm tools update` automatically
registers the canonical Goobits repository when it is missing. Tool updates
activate inside an already-running environment and do not require a base
rebuild. Read credentials travel to the guest over standard input rather than
command arguments. Collection activation merges individual skills into an
existing agent skill directory, preserving unmanaged personal and system
skills. Published binary tools and collections become eligible only in projects
that configure them. Reconciliation selects the guest OS/architecture artifact,
verifies its digest before extraction, installs one immutable release, and
atomically updates its configured links. Normal shell reconciliation adopts an
automatic update without approval; an already-running agent session is not
hot-reloaded.

```bash
vm tools update
vm tools update agent-skills another-tool
vm tools update agent-skills --to projects-dev --to typemill-dev
```

These are the common all-tools, selected-tools, and selected-environments
forms. See the [CLI reference](cli-reference.md#managed-tools) for every update
option.

With no names, `update` uses every running managed Docker environment's own
configured tool selection. Positional names filter those selections and never
make an unconfigured tool eligible. Each `--to` accepts one exact Docker,
Podman, or Tart environment name. Stopped environments are ignored unless
`--include-stopped` is explicit; then VM starts selected stopped environments in
place. A tool absent from every successfully loaded target is rejected instead
of being installed outside configuration. The compatibility `--fleet` form
uses the same execution path while retaining all-state selection, including
starting stopped matches; `--to` is the exact-target syntax.

Omitted versions track the latest release. Explicit semantic versions remain
pinned. An explicit `update` installs every eligible selected change without a
checklist. A persisted `off` policy disables newer-release upgrades, but not a
required first install or pinned-version repair. Normal startup never waits for
the registry, an update prompt, a guest download, or base-owned Codex repair. It
launches only cached automatic tool work and the Vibe runtime probe/repair in
the background. Prompt-policy upgrades remain pending for an explicit `vm tools
update`. The
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

`vm tools update [<tool>...] [--to <environment>]...` is also the idempotent
upgrade reconciliation entry point. For Docker it regenerates current Compose
metadata and updates only a missing or stale `package-edge` sidecar with
`--no-deps`. For Linux Tart it reconciles only the guest edge container. Both
paths preserve the edge cache named volume and leave the primary environment
and base image intact. Both also repair managed client files in place so a new
shell no longer depends on the primary container's creation-time environment.
`vm ssh` and `vm exec` perform that package-edge and client-file repair before
entering the guest or running the requested command. Tool and Codex
reconciliation remains shell-specific.
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

The default update discovers running managed Docker environments and loads each
target's owning configuration before reconciling its shared package routing,
base-owned Codex runtime, and managed-tool selection. It does not project the
invoking project's application services or tool policy onto unrelated
environments. The compatibility `--fleet` form retains its former provider and
pattern behavior and includes stopped matches for existing automation.

Administrative package/tool commands are host-only. Checkout, show, and release
are intentionally guest-safe. Other commands invoked inside a managed guest
print the exact shell-safe host command, for example `Run on the host: vm tools
update --to dev`.

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
- Read, controller, reviewer, build, rollout, release, and publish credentials are
  separate.
- Guest workflow reads are filtered to the capability's assigned consumer;
  shared package and tool catalogs remain readable.
- Project and integration agents never receive Git credentials.
- Project environments consume registry protocols and never mount registry
  volumes.
- Worktrees are isolated by checkout or rollout ID.
- Repository binary commands run as an unprivileged user in a no-egress builder.
  They can reach only the appliance's credential-free read-only package edge,
  not public registries or arbitrary network destinations.
  The narrow queue credential is mounted beneath a root-only directory because
  Docker Desktop does not consistently enforce Compose secret modes; repository
  commands cannot traverse that boundary or read any release, publish, or Git
  credential.
- Canonical host paths never enter the appliance. Authorized project agents can
  submit their clean committed workspace as an immutable bundle; only
  credential-isolated appliance jobs can process that bundle or publish private
  artifacts.
- Receipts contain identities, commits, digests, outcomes, and timestamps—not
  secrets.

On the controller host, `vm packages status` prints one result: `healthy`,
`degraded`, or `action required`. Use `vm packages doctor --fix` for safe
repairs and a precise remaining action when deterministic repair is impossible.
