# CLI provider acceptance

Live acceptance on 2026-09-25 and 2026-09-26 uses disposable project configurations and HOME,
with Docker resources removed by exact test project labels. The reusable command is:

```sh
VM_ACCEPTANCE_BIN=/tmp/vm-rust-target/release/vm \
  bash scripts/internal/test-provider-workflow-docker.sh
```

The binary is built from this checkout, starting at `37658c3d`, with:

```sh
CARGO_TARGET_DIR=/tmp/vm-rust-target cargo build --manifest-path rust/Cargo.toml \
  -p goobits-vm --release --all-features
```

Host runtime: Docker Desktop 4.92.0 (240144), Docker Engine/client 29.8.0,
Compose v5.5.1, darwin/arm64 client and linux/arm64 server, context desktop-linux.
Podman and Tart were initially unavailable. Both were subsequently installed for
isolated live acceptance; their results are recorded separately as checks complete.

## Regressions found during live acceptance

- Strict configuration validation rejected supported `vm.uid` and `vm.gid`.
  Their schema entries and integer typing regression assertions were added.
- An environment-local PostgreSQL declaration registered the global PostgreSQL
  service. Shared lifecycle selection now only uses user-enabled shared services,
  with a project-local service taking precedence. A test verifies that local
  PostgreSQL never registers, starts, or stops a shared service.
- Starting an ordinary environment without selected managed tools required package
  infrastructure. Deferred activation now avoids that unnecessary controller lookup.
- Plugin installation printed a literal `{type}` placeholder. Message substitution
  now recognizes raw Rust identifier arguments.
- Snapshot creation looked for Compose in the project root although managed
  environment Compose files are stored in the generated configuration directory.
  Snapshot operations now select the runtime's recorded Compose file, save a
  normalized configuration using unique committed image tags, and verify loaded
  image IDs before stopping the target.
- Snapshot capture excluded VM-owned named volumes because it assumed default
  Compose volume names. Capture now checks exact configuration/instance ownership
  labels and verifies runtime labels; shared volumes and host binds are excluded.
  Captured volume archives use stock Alpine gzip/tar, and restore validates archive
  contents before stopping, replaces captured contents, and preserves volume labels.
- Noninteractive exec used an interactive login shell, adding Bash job-control
  diagnostics to stderr. It now uses a noninteractive login shell while explicitly
  initializing managed tools/settings.
- Snapshot subprocess progress used stdout, corrupting structured mutation output.
  Provider progress now goes to stderr.
- Concurrent package cleanup could remove a volume between global storage listing
  and inspection, failing an unrelated storage operation. Inventory now skips only
  an exact provider not-found response for that resource; other errors still fail.

Earlier diagnostic runs exposed the schema, unnecessary package infrastructure,
and snapshot routing failures described above. Those runs cleaned their test-owned
containers and volumes; existing host projects were not removed. Temporary run
logs and fixture directories are removed after their results are recorded here.

## Tart regressions found during live acceptance

Tart 2.38.0 was installed with user authorization. A disposable Linux guest uses
separate `HOME` and `TART_HOME`, with image
`ghcr.io/cirruslabs/ubuntu:latest` (manifest SHA-256
`e1814edfeddabeaed5e6bdc646445c96701618d6a2a3d0fe993b0c2914417c07`,
published 2026-08-29). The image and guest-agent usage follow the
[official Tart quick start](https://tart.run/quick-start/).

- Internal Tart setup markers leaked into `vm --quiet exec` stderr. Setup now
  captures both streams, returns command output separately, and includes captured
  diagnostics on failure. A test checks successful output and exit-19 failure
  context; live exact-byte acceptance verifies the resulting behavior.
- Native snapshot restore successfully replaced the guest but left its managed
  runtime directory identity stale, so the next start failed. Restore now validates
  exact ownership and the existing receipt before modifying the guest, then refreshes
  only its device/inode identity after successful restore. The configuration
  fingerprint is retained. Tests verify identity replacement recovery, continued
  configuration-drift detection, and refusal of a different owner.

- Native-only export assumed a container `images` directory existed. Export now
  copies the provider components that are present, after metadata validation has
  verified every required archive. The native-only regression test passes.
- Tart environment discovery inferred project ownership only for `dev` and
  `staging` suffixes. It now reads the project from the recorded owning
  configuration, allowing arbitrary declared environments such as `peer`.

- Tart file uploads and other explicit-stdin transfers omitted `tart exec -i`,
  silently producing empty uploaded files. These paths now attach stdin without a
  terminal; uploads use a direct file stream instead of a shell pipeline. A test
  verifies binary bytes, paths with spaces, and guest failure propagation.

The initial data fixture wrote a marker and immediately stopped the guest without
flushing it. The guest ext4 mount reported `commit=30`; those unflushed bytes were
absent after restarting. A control using `printf persistent > file; sync`, followed
by `vm stop dev`, `vm start dev`, and `vm exec ... cat file`, preserved the marker.
The final snapshot fixture explicitly calls `sync` before each stop so its data
assertions concern durable disk contents. Stopped Tart snapshots do not capture
guest RAM or unflushed filesystem buffers.

## Broader Docker diagnostic

A release-binary diagnostic run that omitted the blocked snapshot section passed
in `vm-provider-acceptance.34ZmeD` (project `vm-provider-acceptance-89776`). It
verified actual PostgreSQL table contents after a two-member backup/restore;
credential redaction; export overwrite rejection; peer-environment database
routing rejection; restricted Redis service plugin activation; DNS tunnel PING,
idempotent open and close; copy contents/overwrite; exec cwd; nonterminal shell
rejection; grouped/JSON Lines fleet execution with child exit 17 and aggregate
failure; logs final event; doctor; restart/stop; and instance-owned data removal.
The Redis fixture bypasses its standard root entrypoint's user-switch operation,
which needs capabilities intentionally unavailable to restricted plugins.

A subsequent complete diagnostic run passed in `vm-provider-acceptance.G6fPxn`
(project `vm-provider-acceptance-16865`) using the rebuilt debug binary. Both
compression formats restored actual rootfs and volume marker data. Missing image,
mismatched image identity, and corrupt volume archive preflights preserved the
running environment; valid retries succeeded. Corrupt portable archive import
failed before installation and a valid import reused the rejected name. The run
also passed exact exec stdout/stderr and decoded fleet/log payload assertions,
fleet stop/start, retained storage reclamation, protected-storage rejection,
and tunnel listing/closure after environment removal.

Final release acceptance includes structured snapshot restore output verification.
The debug pass does not substitute for that release-binary run.

A first release run in `vm-provider-acceptance.kdsFVY` passed snapshot restore JSON,
actual restored rootfs and volume data, archive checksums/identity, database,
tunnel, exec/log payload, and fleet checks. It exposed the concurrent storage
inventory race above during late cleanup; it is recorded as a failed run, with
all test runtime resources cleaned. The runner now also checks restore ownership
against an existing peer environment and verifies that peer's data stays unchanged.

## Earlier Docker acceptance

**Passed**, exit 0, completed 2026-09-26 00:14:23 +08:00. The full command above
ran the release binary against project `vm-provider-acceptance-17697`, using the
pinned Node 22 fixture (Node v22.23.2, NVM v0.40.3, pnpm 10.12.3). The fixture's
real toolchain reconciliation also passed twice with Docker networking disabled,
reporting `VM_NODE_TOOLCHAIN_CURRENT=1` both times. This avoids repeated remote
Node installation when project detection requests a toolchain despite
`bootstrap.dependencies: false`.

The run identifier was `vm-provider-acceptance.tMyTTw`; its transcript ended with
`PASS Docker provider workflow` and `Acceptance exit=0`. Logs, structured results,
and actual data assertions were reviewed before temporary artifacts were removed.
The reusable runner retains the assertions described below; this document records
the result without depending on disposable log paths.

Tested revision identity:

- Base commit: `37658c3d1100b0d68da1bc867228823aa3e70dd4`, plus this task's changes.
- Recorded implementation patch SHA-256 (`rust`/`configs` diff against that base):
  `500283c73c18563dcf6be15c748e6e0ca3618e5ca15a47e6588127f829b8ecf7`.
- Release binary SHA-256:
  `8bb8be321ead6ca9c79aad888a3a45a266e03a7c346863198f91ca54f98b9884`.

The final run passed actual rootfs and volume restoration from both gzip and
uncompressed exports; format magic and manifest checks; archive checksum and
Docker image identity rejection; failed-restore recovery; overwrite and immutable
name behavior; exact project ownership; and restore refusal against an existing
peer with its marker unchanged. Snapshot restore JSON parsed as one result.
Environment, fleet, real log payload, database backup/data restore, plugin, tunnel,
and storage checks all passed, including fleet stop/start, orphan tunnel closure,
disposable-volume reclamation, and kept/in-use volume refusal. A separate byte
comparison of the recorded exec streams confirmed exact stdout/stderr contents.

Cleanup removed all provider-test containers, volumes, networks, committed
snapshot images, fixture images, and newly built environment image tags. The audit
also found custom-image tags from earlier diagnostics; these exact test-owned tags
were removed, and the runner now removes its custom-image tag during cleanup. A
read-only post-run audit on 2026-09-26 found no remaining provider-test runtime
resources and confirmed all **10 original container IDs, 22 volume names, and
9 network IDs** remained present. Shared downloaded base images and build caches
were retained. Bulky temporary fixtures, archives, logs, and diagnostic helper scripts are removed
after review; this document and the reusable runner retain the acceptance evidence.

## Final Docker acceptance after shared provider fixes

**Passed**, exit 0, completed 2026-09-26 02:09:12 +08:00. The reusable command
above ran project `vm-provider-acceptance-5067`, evidence identifier
`vm-provider-acceptance.m6rPUF`, using a pinned release binary with SHA-256
`1454f4cbd755447fa245894ad756e41ad7f1edf20a7d9c1e071280bc3dbb20e0`.
Its tracked implementation diff against `37658c3d` (`rust` and `configs`) had
SHA-256 `862b235d22c29b1515ce0cfc981b4a1e49ec17c3411cc7700b8c61611d227ba9`.
The transcript ended with `PASS docker provider workflow` and `Acceptance exit=0`.

This pass repeats all Docker coverage above after the Podman-driven shared
snapshot fixes: exact-container pause/resume, nonempty captured services, and
empty-snapshot restore rejection before mutation. It verifies actual rootfs and
volume data through gzip and uncompressed archives, and immediate guest access
after quiesced capture. The complete environment, database, tunnel, plugin, fleet,
log, and storage assertions passed. Cleanup removed the run's resources and image
tags; maintained evidence replaces the temporary fixture/archive/log directories.
The final read-only audit again preserved all 10 original container IDs, 22
original volumes, and 9 original network IDs, with no package/provider test
resources or custom images remaining. Seven unrelated derived images belong to
preexisting projects and were retained.

An earlier diagnostic (`vm-provider-acceptance.7x29n8`, project `85892`) passed
through fleet checks, then truthfully failed storage inventory because the newly
installed, unrelated Podman runtime had no connection in its isolated HOME. The
runner now excludes unselected provider discovery and isolates TART_HOME, while
preserving the selected engine's explicit connection. It also copies the binary
at run start so concurrent rebuilds cannot change the tested revision.

Podman passed with its isolated machine removed; see [Podman evidence](cli-podman-acceptance.md). Tart completion is recorded below.
Their unchecked cross-provider tasks remain in the active proposals until actual
acceptance passes; Docker passing alone does not complete provider acceptance.


## Final Tart acceptance — passed

Starting revision: `37658c3d`, with the fixes in the commit containing this log.
Host and installed runtime versions are recorded in [the acceptance log](cli-acceptance.md).
The isolated project was `vm-tart-acceptance-gxks5qxe`, with two Linux guests,
`dev` and `peer`, each using 2 CPUs, 2 GiB memory, and a 20 GiB disk.
Docker and Podman discovery was excluded using fixture-local executable shims.

Acceptance ran in resumable phases in the same disposable fixture. The native
snapshot phase passed at `2026-09-25T18:24:35Z`, using release binary SHA-256
`1454f4cbd755447fa245894ad756e41ad7f1edf20a7d9c1e071280bc3dbb20e0`.
The stdin repair was tested live with binary
`4822a64df894ba1aaea19a333f3c9024c4ca4eab0f4f66c7a411e142fbfef734`.
The final log/lifecycle/storage phase passed at `2026-09-26T07:17:13Z`, using
`522b26c0e9efc4d25e8cad9e6ca277175a8a927cebfdc78f63169eeca48df605`.
Later changes affected Tart stdin and log handling, not the native snapshot path.

The tested phase commands and assertions are consolidated in
[`test-provider-workflow-tart.sh`](../../scripts/internal/test-provider-workflow-tart.sh).
The consolidated runner was syntax-checked; acceptance evidence comes from its
individual live phases, not a claim that the newly assembled runner ran end to end.
To repeat the full workflow:

```sh
VM_ACCEPTANCE_BIN=/tmp/vm-rust-target/release/vm   bash scripts/internal/test-provider-workflow-tart.sh
```

Verified coverage:

- Stopped snapshot creation, gzip/none export and import, overwrite rejection and
  explicit export overwrite, exact restored marker contents, and deletion of data
  written after capture. Running capture was refused.
- Checksum/native digest rejection, invalid native image rejection even with its
  checksum updated, and successful retry after failed import/restore. Project and
  environment ownership checks preserved the original guest and peer marker.
- Arbitrary environment creation; start, restart, fleet stop/start; doctor and
  status; root execution, cwd, exact stdout/stderr, child exit 17, stopped execution
  refusal, and nonterminal shell refusal.
- Copy upload/download exact bytes and overwrite; grouped and JSON Lines fleet
  output; two child exits 17 producing aggregate exit 1 and two failed results.
- Actual application bytes in JSON Lines logs and the final event; human log tail
  bytes; prompt rejection of unsupported service filtering.
- Database routing/backups, service plugins, and tunnels explicitly reject Tart
  as unsupported. Their supported live data workflows passed on Docker and Podman.
- Storage inventory identified exactly the two owned VMs. Removal was refused
  while the owner configuration existed, then succeeded for both unreferenced VMs
  after the test temporarily moved its own configuration.

The last regression was human log handling: the inherited method ignored `--tail`,
`--follow`, and `--service`, calling an unconditional `tail -f`. Human logs now use
Tart's existing byte-preserving record path, sharing option validation with JSON
Lines. The test verifies unsupported service rejection and binary tail contents;
live acceptance verifies human output terminates with the requested bytes.

Cleanup verified no local VMs or acceptance processes remained. The isolated OCI
cache, snapshots, archives, pinned binaries, logs, scripts, active-fixture pointer,
and guest launcher logs were then removed. Installed runtimes and shared build
caches were retained. No host DHCP configuration or existing projects were changed.
