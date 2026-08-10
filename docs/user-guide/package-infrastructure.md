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
Dedicated Linux Tart VM
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
Docker project             Tart project
versioned packages         versioned packages
or one leased checkout     or one leased checkout
```

The Mac stores controller configuration and launches Docker or Tart. It does
not clone package repositories, run package checks, build releases, or publish
artifacts. Project agents never receive Git credentials or registry storage.

## Start the Appliance

```bash
vm packages up
vm packages doctor
```

On its first run, macOS selects Tart and prepares the versioned Linux base
automatically. Later runs reuse the stored runtime. Other platforms select
Docker. Use `--runtime docker` only when every consumer is Docker-based, or
`--runtime tart` to switch explicitly. The Docker gateway is loopback-bound and
is deliberately rejected for Tart consumers. The Tart appliance exposes its
gateway on the private VM address so both providers can reach it.

VM injects the gateway and a read-only token through npm, Cargo, and pip
environment settings whenever it creates or starts a project environment. It
also exports `VM_OCI_MIRROR`; Linux Tart guests with managed Docker activate
that mirror in Docker Engine automatically.
Projects keep ordinary versioned dependencies; no local/remote branch belongs
in application code.

The OCI cache shares the private gateway's `/v2/` route but has its own named
volume. It accepts pulls only; Distribution proxy mode rejects pushes. VM never
rewrites the Mac Docker daemon. Docker-hosted projects may opt their external
daemon into the exported mirror, while Tart's nested Linux daemon is configured
inside the guest.

## Register Sources and Consumers

Store Git and optional remote-registry tokens as controller secrets:

```bash
vm packages auth --token-file /secure/input/git-token
vm packages auth --ci-token-file /secure/input/registry-token
```

The input files are read once. The tokens are exposed only to the scoped
service or release job that needs them.

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

Path registration detects `package.json`, `Cargo.toml`, or `pyproject.toml`,
then reads each repository's `origin` and default branch. Every supplied path
must be a Git repository root. Use `--ecosystem` when a repository intentionally
contains more than one supported manifest. Discovery only registers metadata;
it does not copy, mount, build, or publish the local source.

Supported ecosystems are `npm`, `cargo`, and `python`. A package has one
canonical repository and immutable published versions.

## Register And Consume Tools

Tool definitions use the same private appliance but remain separate from
language-package protocols:

```bash
vm tools register codex \
  --kind binary \
  --repository https://github.com/example/codex.git
vm tools register agent-skills \
  --kind collection \
  --repository https://github.com/example/agent-skills.git
vm tools list
vm tools show agent-skills
```

Trusted infrastructure release jobs publish target-specific, immutable
archives and receipts; project agents cannot publish them. Projects select
versions through the one-level `tools:` map in `vm.yaml`. A collection such as
`agent-skills` is one atomic version, even when it activates into several agent
directories.

```bash
vm tools refresh
vm tools status [environment]
vm tools update [environment]
vm tools update [environment] --all --background
```

Omitted versions track the latest release. Explicit semantic versions remain
pinned. Normal startup never waits for the registry or an update check.

## Develop and Release a Package

Run checkout from the selected consumer project:

```bash
vm packages checkout auth \
  --agent agent-17 \
  --task "fix token refresh"
```

The work service records the canonical base commit, creates a unique branch,
and returns a bundle to an isolated checkout under the project environment's
`/tmp`. Only that project receives the unpublished override. Other consumers
remain on their published versions.

Commit the intended package changes inside the project environment, then run:

```bash
vm packages submit <checkout-id>
vm packages integrate <submission-id>
vm packages publish <submission-id> --push-source
```

Submission validates the exact bundle and selected consumers, then launches a
credential-free ephemeral reviewer. Integration is serialized against the
latest canonical commit. Publication requires explicit source-push authority,
verifies the semantic version bump, pushes the matching commit and tag, and
publishes the same immutable artifact locally and to the configured CI
registry. A successful release closes and removes only its temporary checkout.

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
