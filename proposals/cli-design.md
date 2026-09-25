# VM CLI design

Status: implementation in progress. This document specifies the intended product
interface; the public reference describes the commands currently available.

Implemented so far: project initialization and explicit project selection;
project-owned environment declarations with stopped `create` and retryable
`start`; deterministic named targeting and repeated `exec --env`; project-scoped
snapshots with archive checksums and mount coverage; grouped package,
configuration, database, and tunnel commands; config field provenance and
validated atomic mutations; explicit tool targeting; exact project ownership
checks for fleet actions and tool updates; safe
Docker/Podman storage inspection; controller-only package, consumer, and tool
registration removal with retained history; restricted, staged plugin installation;
and removal of dynamic top-level tool commands.
The public reference records the current spelling and capability limits.

Still to deliver: runtime drift detection, complete fleet removal and data
ownership, provider-native snapshots beyond Compose, package Docker acceptance,
per-environment database routing, general tunnel endpoints, typed plugin preset
capabilities and service activation, Tart/file storage ownership, structured output,
exit codes, receipts, generated reference, and genuine per-command plans. These
require backend and acceptance work before this proposal can be marked complete.

## Product contract

`vm` manages reproducible development environments and the private packages and
tools used inside them. Routine work should require a short command. Automation
should use the same operations with explicit targets and structured results.

The design has one canonical spelling for each operation, deterministic target
selection, explicit ownership of stored data, and consistent failure behavior.
Commands describe user tasks; backend details appear only where a user must make
a meaningful choice.

This is a complete target interface, not an instruction to build every feature
at once. Package implementation and acceptance remain owned by the active
[package experience tracker](vm-package-experience-tracker.md). Broader provider,
package-manager, and public-registry support requires separate scope and acceptance.

## Concepts and grammar

| Concept | Meaning |
| --- | --- |
| Project | A registered configuration root with a stable identity. |
| Environment | A named runtime belonging to a project, with a recorded provider. |
| Provider | A runtime backend, such as Docker, Podman, or Tart. |
| Snapshot | A named, immutable capture with recorded provider and data coverage. |
| Package | A registered library published to the private package service. |
| Tool | A registered executable or executable collection selected for environments. |
| Consumer | A registered repository and its declared package dependencies. |
| Source | A registered repository root used by a package or tool. |
| Checkout | An isolated editable source workspace owned by one environment. |

Environment actions live at the top level. Resource groups use plural nouns:
`snapshots`, `packages`, `tools`, `tunnels`, `secrets`, and `plugins`.
`db` is the canonical database group. `config` and `system` name configuration
and installation concerns. Familiar short names and frequent workflows take
priority over uniform grammar.

Use `list` for collections, `show` for one resource, `status` for live state,
`remove` to delete a registered resource, and `set`/`unset` for configuration.
Domain verbs such as `release`, `restore`, and `enable` retain their precise
meaning. Do not add synonymous commands or accept abbreviated spellings.

Resource names are positional. Commands whose only positional resource is an
environment take its name directly: `vm start dev`, `vm shell dev`. Commands
with another resource or a process payload use `--env`: `vm snapshots restore
clean --env dev`, `vm exec --env dev -- cargo test`. Each command has exactly
one explicit environment-selection syntax; a positional environment command
does not also accept `--env`.
In the inventory below, uppercase names are arguments, square brackets indicate
optional arguments, and `...` indicates repetition.

## Context and targeting

- Discover the project from the nearest configuration root above the working
  directory. `--project PATH_OR_ID` explicitly selects another project.
- Select an environment with the command's positional name or `--env NAME`
  inside that project. Without an explicit selection, use the configured
  default, or the sole declared environment. Multiple candidates
  without a default are an error listing the choices.
- Never select the first running environment, remember a hidden last target, or
  silently fall back to another project. Discovery commands can run without a
  project; environment operations require a resolved project.
- `--config PATH` selects an explicit project configuration file and conflicts
  with `--project`. `--profile NAME` selects a configuration overlay for this
  invocation. Neither changes persistent defaults.
- Existing environments use their recorded provider identity. `--provider`
  chooses a backend during creation; a fleet filter uses `--match-provider`.
- Fleet-capable environment commands accept positional names (`vm stop dev test`);
  `exec` and `tools update` accept repeated `--env NAME`. All fleet-capable
  commands accept `--all-envs` within one project, mutually exclusive with named
  targets. `--match NAME_GLOB` and `--match-provider PROVIDER` require
  `--all-envs`. Empty selections fail. The resolved set is frozen before execution.
- Fleet support is limited to `start`, `stop`, `restart`, `status`, `remove`,
  `exec`, and explicit tool updates. Shells, copies, database changes, restores,
  and streaming logs use one environment.
- Destructive fleet operations display the full target set and require
  confirmation. They never extend beyond the selected project.

## Command inventory

### Environments and daily work

```text
vm init [PATH]
vm create NAME --provider PROVIDER (--image IMAGE | --snapshot SNAPSHOT)
vm start [ENV... | --all-envs]
vm stop [ENV... | --all-envs]
vm restart [ENV... | --all-envs]
vm list [--all-projects]
vm status [ENV... | --all-envs]
vm remove [ENV... | --all-envs] [--delete-data]
vm shell [ENV] [--cwd PATH]
vm exec [--env NAME] [--cwd PATH] [--user USER] -- PROGRAM [ARG...]
vm logs [ENV] [--service NAME] [--follow] [--tail N]
vm copy [--env NAME] SOURCE DESTINATION
vm doctor [ENV] [--fix]
```

`init` writes a minimal project configuration without starting services or
overwriting an existing file. `create` persists an environment declaration in the
project configuration and provisions it stopped; resource settings include `--cpu`, `--memory`, and repeated
`--mount`. Creation fails if the name exists. Provider capabilities are checked
before provisioning. Provisioning failures leave a visible declared environment
with its failure state, so `start` can retry without an orphaned runtime.

`start` ensures the selected declared environment exists and is running. It
creates a missing declared environment from its configuration. It reports
configuration drift on an existing runtime instead of silently recreating it.
`stop` is idempotent; `restart` preserves runtime configuration and data.

`list` defaults to environments in the current project; outside a project it
lists registered environments with project identities. `--all-projects` makes
that wider read scope explicit inside a project. `status` shows observed state,
health, provider, configuration drift, endpoints, and pending tool activation.

`remove` deletes the runtime while preserving persistent volumes and snapshots.
`--delete-data` also deletes exclusively owned persistent environment data;
shared data cannot be deleted through this command. Retained data remains
discoverable through `system storage list` with its owner and reclamation command.
Environment declarations remain configuration, so a subsequent `start` can
recreate a declared environment. Output states that fact.

`shell` opens an interactive shell and requires a terminal. `exec` passes an argv
array without shell interpolation; shell syntax requires an explicit shell, such
as `vm exec -- sh -lc 'make && make test'`. Neither silently starts a stopped
environment. `copy` uses `host:PATH` and `env:PATH`; exactly one endpoint is local
and one belongs to the selected environment. Absolute and relative path rules
are documented, and overwrite requires `--overwrite`.

`doctor` performs read-only checks by default. `--fix` describes and applies
supported repairs, preserving unrelated configuration and data. It is not a
general cleanup or reset command.

### Snapshots and portable archives

```text
vm snapshots list [--env NAME]
vm snapshots show NAME
vm snapshots create NAME [--env NAME] [--description TEXT] [--quiesce]
vm snapshots restore NAME [--env NAME]
vm snapshots remove NAME
vm snapshots export NAME --output FILE [--compression FORMAT]
vm snapshots import FILE --name NAME
```

Snapshot names are unique within a project. Creation records source identity,
provider, architecture, image identity, consistency level, and exactly which
disks and volumes are included or excluded. Host bind mounts are never implicitly
captured. Unsupported quiescing or restoration fails before modification.

Restore replaces captured state in the selected environment after confirmation;
it does not rename that environment or modify unrelated volumes. Import validates
the archive and registers a snapshot; `create --snapshot` instantiates it.
Import and creation never overwrite a name. Export writes atomically and requires
`--overwrite` to replace a destination file. Archives carry checksums and explicit
format versions. Provider portability is a declared capability, not an assumption.

### Packages and shared release work

```text
vm packages register PATH_OR_NAME... [--repository URL] [--branch BRANCH]
vm packages list
vm packages show NAME
vm packages remove NAME
vm packages open SOURCE
vm packages checkout SOURCE
vm packages release [--receipt ID]
vm packages cancel
vm packages consumers register NAME --repository URL [--branch BRANCH]
    --dependency PACKAGE@VERSION...
vm packages consumers list [--package NAME]
vm packages consumers show NAME
vm packages consumers remove NAME
vm packages consumers drift [--package NAME]
vm packages consumers retry NAME
vm packages service init --source-root PATH
vm packages up [--engine ENGINE] [--port PORT]
vm packages down
vm packages service status
vm packages service doctor [--fix]
vm packages service backups list
vm packages service backups create [--name NAME]
vm packages service backups restore NAME
vm packages service backups remove NAME
vm packages auth login [--token-stdin | --token-file FILE]
vm packages auth status
vm packages auth logout
```

Registry discovery derives ecosystems and roots from manifests. Explicit
`--ecosystem` resolves ambiguity; `--recursive` opts into repository traversal.
Registration reports the discovered roots before applying a multi-root change.
Registration removal never deletes published versions or source repositories.

`open` opens the attested source root in its owning workspace environment and
can start that owner. `checkout` creates an isolated editable checkout inside the
requesting managed environment. These are distinct ownership choices. Neither
route silently substitutes for the other, and neither requires host-side builds.

`release` derives the registered source and submitted commit from the working
directory. It performs review, integration, build, publication, and activation
through the shared durable release pipeline. It also releases registered tool
sources. `cancel` closes the current editable submission and cleans its owned
workspace; it does not unpublish completed releases. Ambiguous or unregistered
working directories fail with a concrete next action.

Package and tool release infrastructure has one owner. Do not introduce a second
`tools release` pipeline or a general-purpose jobs subsystem. Durable receipts
identify retries and support resuming a release without publishing different
bytes for the same version. Output separates publication, consumer review work,
and environment activation. A stopped environment may be successfully deferred;
a failed activation remains a visible failure requiring retry.

Consumers are registered repositories; `--package` finds dependents of one
package. Consumer updates produce reviewed dependency changes. Retry resumes
eligible failed work and reports whether anything was scheduled. It does not
claim a reviewed update has merged merely because a branch was created.

`packages up` ensures the configured package service is running; `packages down`
stops it while retaining its data. Both are idempotent. These frequent lifecycle
actions stay directly under `packages`. Inspection, diagnostics, initialization,
and backups stay under `packages service`. Backups state their contents and
consistency; restore requires the service
to be quiescent and checks format compatibility before replacing data. Authentication
uses an interactive provider flow by default, with explicit secure token inputs
for automation. Credentials are never exposed by `status`.

### Tools

```text
vm tools register NAME --repository URL [--branch BRANCH] [--kind KIND]
vm tools list
vm tools show NAME
vm tools remove NAME
vm tools refresh [NAME...]
vm tools enable NAME...
vm tools disable NAME...
vm tools status [--env NAME]
vm tools update [NAME...] [--env NAME... | --all-envs] [--include-stopped]
```

Registration defines sources; enablement defines the controller-wide desired tool
selection; update reconciles selected versions into environments. `refresh`
refreshes source metadata and never activates tools. `list` shows registration
and desired selection; `status` shows desired and observed versions per environment.

Enable and disable reconcile running eligible environments, with stopped
environments deferred. Update defaults to the selected environment; fleet scope
must be explicit. `--include-stopped` records desired updates for stopped targets
without starting them. All these commands report activation outcomes.

Removing an enabled tool fails until it is disabled. Tool executables are exposed
as normal guest executables on `PATH`, with collision validation. Invocation uses
the executable directly inside the guest or `vm exec -- TOOL ...` from the host.
Installed tools cannot introduce new `vm` top-level commands or shadow builtins.

### Configuration

```text
vm config show [--scope project|user|effective]
vm config get KEY [--scope project|user|effective]
vm config set KEY VALUE [--scope project|user]
vm config unset KEY [--scope project|user]
vm config validate
vm config render [--env NAME]
vm config profiles list
vm config profiles show NAME
vm config profiles set-default NAME
vm config presets list
vm config presets show NAME
vm config presets apply NAME... [--scope project|user]
vm config ports [--fix]
```

Reads default to effective configuration; writes default to project configuration
and fail outside a project unless user scope is explicit. Precedence is command
options, selected profile, project configuration, user configuration, then builtin
defaults. `show` identifies value provenance. Effective configuration is read-only.

Types and allowed keys come from one schema. Scalar values use the declared type;
arrays and objects use `--value-json JSON` in place of `VALUE`. Mutation validates
the complete result and writes atomically. Preset application reports conflicts
instead of silently overwriting explicit user settings. `unset` removes one
override and reports the resulting effective value.

`render` displays generated provider configuration with secrets redacted. Profile
default selection persists in project configuration. `ports --fix` only repairs
managed configuration after checking conflicts; it does not stop other processes.
The current key validator rejects unknown fields inside schema-owned objects
while preserving flattened root fields used by extensions. User configuration
currently contributes managed tools; broader user defaults and field provenance
through generated preset values still require acceptance against real projects.

### Secrets, tunnels, and databases

```text
vm secrets list [--scope project|user]
vm secrets status
vm secrets set NAME [--scope project|user] [--stdin | --file FILE]
vm secrets show NAME [--scope project|user] --reveal
vm secrets remove NAME [--scope project|user]

vm tunnels list [--env NAME]
vm tunnels open NAME --local ADDRESS:PORT --remote HOST:PORT [--env NAME]
vm tunnels close NAME [--env NAME]

vm db list [--env NAME]
vm db status NAME [--env NAME]
vm db backups list [--database NAME] [--env NAME]
vm db backups create NAME [--database NAME | --all] [--env NAME]
vm db backups restore BACKUP --database NAME [--env NAME]
vm db backups remove BACKUP [--env NAME]
vm db export NAME --output FILE [--env NAME]
vm db import NAME --file FILE [--env NAME]
vm db reset NAME [--env NAME]
vm db credentials NAME [--env NAME] [--reveal]
```

Secrets default to project scope. `set` prompts without echo on a terminal; scripts
must choose stdin or file input. Values never enter positional arguments, logs,
plans, or ordinary structured output. List and status expose metadata only.
`show --reveal` explicitly writes the value to stdout and disallows `--json`.
Secret rotation replaces the named value atomically.

Tunnels have stable names, bind to loopback by default, and report their owning
environment and actual endpoints. Non-loopback listeners require an explicit
address. Opening an identical named tunnel is idempotent; a conflicting definition
fails. Close affects only the named tunnel.

Database operations use configured service identities and one environment.
Backup creation requires either a database or explicit `--all`; a multi-database
backup reports each member's result and does not claim transactional consistency
across services. Restore requires an unambiguous matching backup member. Reset
means deletion and reinitialization of the named database, with confirmation.
Credentials are redacted unless `--reveal` is explicit. Export destinations require
`--overwrite` if present; imports and restores report their destructive scope.

### Plugins and installation

```text
vm plugins list
vm plugins show NAME
vm plugins install PATH
vm plugins remove NAME
vm plugins create NAME --kind preset|service
vm plugins validate PATH_OR_NAME

vm system info
vm system update [--version VERSION]
vm system uninstall [--delete-config] [--delete-data]
vm system images build PRESET --provider PROVIDER [--guest-os OS]
vm system storage list
vm system storage remove RESOURCE_ID
```

Plugins extend declared presets and services through validated manifests. They
cannot rewrite builtin command parsing. Installation validates identifiers,
capabilities, and conflicts before making files visible. Any executable extension
mechanism would require its own explicit trust model and proposal.

`system info` reports client, controller, provider, and schema versions with
redacted diagnostic context. Update verifies downloaded artifacts and records
the installed version. Uninstall identifies the exact installation it owns;
configuration and data deletion require separate explicit options. Shared or
unowned resources are never removed as a side effect.

Storage removal accepts an exact discoverable resource ID, checks that it is
unreferenced, and requires confirmation. There is no blanket deletion command
for all provider resources. Base-image builds have explicit provider capability
checks and produce identifiable reusable artifacts.

## Interaction, output, and automation

### Common options

Context options are `--project`, `--config`, and `--profile`. Applicable commands
also expose `--env`, `--json`, `--quiet`, `--no-color`, `--non-interactive`,
`--timeout DURATION`, `--dry-run`, and `--yes`. Help only advertises options the
command actually supports. `--help` and `--version` require no backend access.

`--quiet` suppresses progress, not errors or requested data. `--non-interactive`
disables all prompts. `--yes` answers an ordinary confirmation; it cannot supply
a missing target, bypass validation, overwrite immutable artifacts, or replace
explicit deletion options. There is no universal safety-bypassing force flag.

### Output contract

- Stdout contains the requested result; stderr contains progress, diagnostics,
  and prompts. Human output includes units, explicit states, and actionable errors.
- `--json` emits one versioned envelope containing `schema_version`, `command`,
  `ok`, `data`, and `errors`. Resource objects use stable IDs, explicit units, UTC
  timestamps, and documented nullable fields. Sensitive fields are omitted.
- Fleet results contain one result per frozen target and an aggregate outcome.
  Human fleet execution prefixes output with target identities.
- Long-running structured streams use the separate option `--json-lines`, with
  ordered typed events and a final result event. It conflicts with `--json`.
- `exec` preserves the process stdout and stderr byte streams and does not accept
  JSON output options. Fleet execution requires `--output grouped|json-lines`;
  JSON Lines encodes arbitrary process bytes and labels each target and stream.
- `logs --json-lines` wraps records with environment, service, timestamp when
  available, and payload; it never guesses that application text is valid JSON.

Machine-readable fields are a published API with schema fixtures. Human table
layout is not a scripting interface. Errors include a stable code, concise
message, relevant target, and concrete next action. Debug traces are opt-in and
redacted.

### Exit status and interruption

| Exit | Meaning |
| --- | --- |
| 0 | Requested outcome achieved, including documented deferred work. |
| 1 | Operational failure; fleet results explain individual failures. |
| 2 | Invalid arguments or unresolved context. |
| 3 | Valid request blocked by a state conflict or missing precondition. |
| 4 | Authentication or authorization failure. |
| 5 | Deadline exceeded; durable work may still be running. |
| 130 | Interrupted by the caller. |

Single-target `exec` returns the child's exit code. Failure before launching a
child returns 125 with a diagnostic. Signal termination follows shell conventions.
Fleet `exec` returns 0 only when every child succeeds, otherwise 1, with individual
exit statuses in results. Interactive execution forwards terminal size and signals.

### Waiting and durable work

Mutations wait for their documented outcome by default and report progress
promptly. Durable release and reconciliation operations support `--background`,
which returns a receipt only after the service has accepted the work. Simple
local writes do not pretend to support background execution.

A timeout or client interruption does not imply durable work was cancelled.
The final diagnostic states whether work continues and gives a domain-specific
inspection command. `packages release --receipt ID` observes/resumes an existing
release; `tools status` exposes activation receipts. IDs are for diagnosis and
automation, not routine release prerequisites. Repeating an operation uses the
domain's idempotency rules, never a generic blind retry loop.

Publication and activation are distinct outcomes. A normal release waits for
publication and bounded activation attempts; failed required targets yield a
failure result even if publication succeeded. Deferred stopped targets are
reported explicitly. Consumer review branches are reported as pending review.

### Planning and confirmation

`--dry-run` is available on mutations that can produce a meaningful plan. It
resolves context, reads state, validates capabilities, and reports targets,
changes, retained/deleted data, and required preconditions. It performs no writes,
credential refresh, lockfile changes, service starts, or submission creation.
Read-only authenticated inspection is allowed using already available credentials.

Plans mark unknown remote conditions explicitly. They are observations, not
reservations; execution revalidates state. Arbitrary `exec`, interactive shells,
and unsupported provider operations reject dry-run instead of inventing a result.

Destructive actions prompt on a terminal. Without one they fail unless `--yes`
is supplied. JSON mode disables prompts. Explicit high-impact choices such as
`--delete-data` are still required alongside `--yes`.

## Implementation structure

1. Keep argument parsing, domain requests, application services, provider adapters,
   and presentation separate. A handler resolves context once, constructs a typed
   request, invokes the owning service, and renders its typed result.
2. Centralize target resolution, output envelopes, confirmation, cancellation,
   redaction, and capability checks. Do not duplicate these across commands.
3. Share each operation's validation and plan model between preview and execution.
   Provider adapters report capabilities explicitly; unsupported behavior is
   detected before effects begin.
4. Keep release, reconciliation, backup, and storage ownership in their domain
   services. The CLI does not own a second persistence model or job queue.
5. Generate command reference and completion metadata from the command model.
   Keep examples and semantic documentation next to the relevant domain. Internal
   workers use dedicated binaries or private service entry points, outside the
   public command tree.
6. Give every mutation explicit atomicity and retry semantics. Report partial
   outcomes when several independent targets cannot share a transaction.

## Delivery and acceptance

Implement by complete workflow, preserving a small reviewable scope per change:

1. Define the typed command model, context resolution, output/error contracts,
   capability model, and generated reference.
2. Deliver environment lifecycle, exec/shell/copy/logs, and snapshot workflows.
3. Deliver configuration, secrets, tunnels, and database workflows.
4. Apply package/tool interface decisions within the active package tracker;
   retain its release, isolation, and Docker acceptance requirements.
5. Deliver plugin/system administration and storage ownership inspection.

Each workflow is complete when its documented examples work, terminal and
nonterminal behavior agree with this contract, errors identify the failing target,
and cancellation/retry behavior is verified. Tests should cover meaningful
boundaries: ambiguous context, empty fleet selection, provider mismatch, secret
redaction, child exit codes/signals, partial failure, failed atomic writes,
destructive scope, and receipt-backed retries. Command-schema fixtures catch
accidental public interface drift.

Help and generated documentation must agree. Provider acceptance establishes
capabilities per provider; package-manager acceptance establishes supported
manifest and lockfile workflows. Unsupported combinations fail clearly and do
not appear as successful no-ops.

## Example daily session

```sh
vm init
vm create dev --provider docker --image debian:bookworm
vm start
vm status
vm shell
vm exec -- cargo test
vm logs --service api --follow
vm snapshots create before-experiment --quiesce
vm snapshots restore before-experiment
vm stop
```

With multiple declared environments, configure a project default or name the
environment explicitly: `vm start dev`, `vm shell dev`, and
`vm exec --env dev -- cargo test`.

A source release in its owning workspace remains short:

```sh
vm packages open my-library
# In the opened source working directory:
vm packages release
```

Automation makes context and output explicit:

```sh
vm status dev --project /work/app --json
vm exec --project /work/app --env dev -- cargo test --locked
vm stop --project /work/app --all-envs --non-interactive --json
```

## Workflow review

These are design walkthroughs, not runtime acceptance results. They check the
grammar against frequent tasks and the points where terse commands can become
ambiguous.

| Workflow | Commands | Design check |
| --- | --- | --- |
| Work in the default environment | `vm start`, `vm shell`, `vm stop` | No repeated context flags. |
| Switch between declared environments | `vm start dev`, `vm shell test` | The positional name always means an environment. |
| Run a program named like an environment | `vm exec -- dev`, `vm exec --env test -- dev` | The delimiter separates program arguments from CLI options. |
| Stop a selected set | `vm stop dev test` | Named scope is explicit without repeated flags. |
| Stop every project environment | `vm stop --all-envs` | Wider scope is explicit and cannot combine with names. |
| Restore a snapshot into another environment | `vm snapshots restore clean --env test` | Snapshot and environment identities cannot collide. |
| Bring up the package service | `vm packages up` | Frequent infrastructure setup needs no extra subgroup. |
| Inspect and retry consumer work | `vm packages consumers list --package lib`, `vm packages consumers retry app` | Discovery and retry share one resource group. |
| Set a secret | `vm secrets set TOKEN` | Secure input is the default without extra flags. |
| Inspect a database | `vm db list`, `vm db status app --env test` | A short domain name retains explicit resource targeting. |
| Run from automation | `vm status dev --project /work/app --json` | Explicit context and structured output use the same operation. |

The design review resolves positional-versus-payload ambiguity and keeps routine
commands short. Implementation acceptance must still verify these workflows
against the parser and supported providers before the interface is finalized.
