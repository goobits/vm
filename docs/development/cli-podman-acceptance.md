# Podman live CLI acceptance

## Isolation and runtime setup

Acceptance on macOS 27.0 (26A428), arm64, uses Podman 6.1.2 installed on 2026-09-26. The
pre-acceptance Podman connection list was empty. Runtime configuration, machine
data, cache, and connection metadata are isolated under
`/tmp/vm-podman-acceptance`; host default connections and Docker resources are
not used for Podman mutations.

The disposable machine is named `vm-cli-acceptance-20260926`, with two CPUs,
3072 MiB memory, a 30 GiB disk, and rootful container execution. Setup uses:

```sh
export XDG_CONFIG_HOME=/tmp/vm-podman-acceptance/config
export XDG_DATA_HOME=/tmp/vm-podman-acceptance/data
export XDG_CACHE_HOME=/tmp/vm-podman-acceptance/cache
export PODMAN_CONNECTIONS_CONF=/tmp/vm-podman-acceptance/connections.json
podman machine init --cpus 2 --memory 3072 --disk-size 30 --rootful vm-cli-acceptance-20260926
podman machine start vm-cli-acceptance-20260926
```

Keep that launcher session alive until acceptance completes; in an execution
environment that terminates child processes when its shell exits, use this
supervisor immediately after `machine start`:

```sh
trap 'podman machine stop vm-cli-acceptance-20260926' EXIT
while test ! -e /tmp/vm-podman-acceptance/stop-machine; do sleep 5; done
```

In the separate acceptance shell, export the same isolated XDG and connection
configuration variables above, then select the machine's explicit API socket:

```sh
export PODMAN_COMPOSE_PROVIDER=/Applications/Docker.app/Contents/Resources/cli-plugins/docker-compose
unset CONTAINER_CONNECTION
podman_socket=$(podman machine inspect vm-cli-acceptance-20260926 --format '{{.ConnectionInfo.PodmanSocket.Path}}')
export CONTAINER_HOST="unix://$podman_socket"
podman info
podman compose version
```

The recorded socket was
`unix:///var/folders/_c/w_blfps94yjd9wg142pgqw4r0000gn/T/podman/vm-cli-acceptance-20260926-api.sock`.
Selecting it explicitly preserves access when the runner changes HOME/XDG for
each disposable project. After acceptance, create the stop marker, wait for the
supervisor to exit, and run `podman machine rm --force vm-cli-acceptance-20260926`
with the same isolated machine configuration.

The shared provider runner supports an explicit engine without changing its
Docker default:

```sh
VM_ACCEPTANCE_PROVIDER=podman VM_ACCEPTANCE_BIN=/tmp/vm-rust-target/release/vm \
  bash scripts/internal/test-provider-workflow-docker.sh
```

Each attempt records the checkout revision, binary SHA-256, provider/Compose
versions, command results, and cleanup evidence in its disposable evidence root.
The runner isolates project configuration and targets only its unique resource
labels for cleanup.

## Result

The complete Podman provider workflow passed on 2026-09-26 with exit 0.
The tested checkout includes `37658c3d1100b0d68da1bc867228823aa3e70dd4`
plus the acceptance fixes recorded in the final task commit. The pinned release
binary SHA-256 is
`a7fabf0358db4cc5a5719ee091f6f2d5784ffbbcd8aa12e4c2d5293b3437176c`.
The successful evidence root is
`/var/folders/_c/w_blfps94yjd9wg142pgqw4r0000gn/T/vm-provider-acceptance.Jm5hDD`,
with the complete log also at `/tmp/vm-podman-acceptance/workflow-6.log`.
The root retained the exact runner and binary, archive-format checks, restore
JSON, database results, fleet/log records, storage inventories, and cleanup log
until review. Those bulky temporary artifacts were then removed; this report
and the reusable runner retain the acceptance evidence.

Verified live behavior includes:

- Startup, environment settings, exact stdout/stderr, working directory, and copy overwrite behavior.
- Quiesced snapshot creation and verified resume; gzip/none export, overwrite,
  import, and actual rootfs/volume restoration with post-snapshot files removed.
- Project and environment ownership rejection, missing images, empty capture
  metadata without target replacement, native image identity mismatch, corrupt
  volume payload, archive checksum rejection, and successful recovery afterward.
- Database routing, multi-database backups, export overwrite rejection, restored
  row verification, backup removal, and credential redaction.
- Service plugin installation, a live Redis tunnel PING, idempotent reopen,
  conflicting declarations, fleet output and failures, logs, status, doctor,
  restart, stop/start, retained storage ownership, protected storage, removal,
  orphan tunnel cleanup, snapshot removal, and plugin removal.

After the pass, native container and volume inventories were empty. The named
machine was stopped and removed; isolated machine/connection lists and the host
default connection list were all `[]`. The test-owned runtime cache, guest disk,
keys, EFI state, and machine configuration were removed. `~/.podman` was empty.
All temporary logs, acceptance artifacts, and the isolated runtime directory
were removed after review. The stale first-boot socket and gvproxy log were also
removed. There are no live Podman acceptance resources.

## Runtime details and regressions resolved

The tested machine used client/server Podman 6.1.2, Fedora 44 arm64,
kernel `7.1.10-200.fc44.aarch64`, Apple Hypervisor, and Docker Compose 5.5.1 as
Podman's external Compose provider. The Docker API helper was not installed;
Docker's existing socket remains untouched.

The first launcher session ended during first boot; the next boot reported
`useradd: cannot lock /etc/group` from Ignition. The disposable machine was
stopped and recreated from its cached image. A persistent launcher session
keeps the clean machine alive until a test-owned stop marker is created.
This occurred before VM CLI acceptance and is not attributed to VM code.
The original boot log is `/tmp/vm-podman-acceptance/interrupted-firstboot.log`.

An initial workflow attempt (`vm-provider-acceptance.USBy1G`) stopped before
environment creation because Podman Compose searched the isolated project XDG
home for machine metadata. An explicit `CONTAINER_HOST` using this machine's
API socket fixes that setup issue while preserving project isolation.
`PODMAN_COMPOSE_PROVIDER` names the installed Docker Compose executable;
`CONTAINER_CONNECTION` is unset so it cannot override the explicit socket.

The second attempt failed during provisioning, using the pinned binary
`/tmp/vm-podman-acceptance/vm-under-test` with SHA-256
`e055b60103c9e06549c215e9e157a011742905b84d86a5922d7fb3c631d0cf92`.
Its evidence root is `vm-provider-acceptance.T2pMDQ` under the macOS temporary
directory, with a copy of the live log at
`/tmp/vm-podman-acceptance/workflow-2.log`. Provisioning tested only `/.dockerenv`, missed Podman's `/run/.containerenv`,
and treated the container as a full VM. Its atomic `/etc/hosts` update then
failed with `Device or resource busy`. The playbook now sets one `is_container`
fact from either runtime marker, and service tasks use that shared fact.
All 18 provider resource tests pass, and both edited YAML resources parse.
Subsequent attempts verified the corrected provisioning branch.

The third attempt (`vm-provider-acceptance.wTP19h`, log
`/tmp/vm-podman-acceptance/workflow-3.log`) used release SHA-256
`b6e054dfec68f80f9f8712a8f201470bf53066d94d6c18e86eee60c55f76ed9d`.
Provisioning, raw exec output, copy overwrite checks, and snapshot gzip/none
export/import passed. It then detected an empty captured-service list:
Podman's default Compose listing excludes paused containers. Snapshot capture
now queries all container states and rejects empty or disappearing selections.
All three snapshot-create tests pass, including paused rootfs capture and
failure cases. The workflow now explicitly asserts that captured rootfs images
exist. The final passing run verifies live restore as well.

The fourth attempt (`vm-provider-acceptance.Bspy3k`, log
`/tmp/vm-podman-acceptance/workflow-4.log`) used release SHA-256
`7892c49e4f29297a210ea8b84caef918c414cf0735390fefa218bfeb5fa2d535`.
It captured all three service images and passed both archive formats and imports.
A later data check found the environments still paused: Podman's Compose
`unpause` command had returned success without resuming them. Snapshot quiescence
now freezes exact container IDs, uses native pause/unpause, verifies resulting
states, and attempts recovery for every changed container after partial failure.
Initially stopped/paused containers retain their state. All 36 snapshot tests
and targeted Clippy pass, including three state/recovery tests. The final run
verifies native pause/resume and restored data on the live provider.

The fifth attempt (`vm-provider-acceptance.N5tSGu`, log
`/tmp/vm-podman-acceptance/workflow-5.log`) used release SHA-256
`1454f4cbd755447fa245894ad756e41ad7f1edf20a7d9c1e071280bc3dbb20e0`.
Quiesced capture resumed all running containers. Both imported archive formats
restored the actual volume and rootfs markers and removed post-snapshot files.
Missing images, empty service metadata, incorrect native image IDs, corrupt
volume archives, and archive checksums failed safely; subsequent operations
recovered. Project/environment ownership checks, databases, tunnels, fleet
execution/logs, and lifecycle checks passed. Storage inventory then rejected
Podman's native bare 64-character image ID. Inventory now accepts that native
Podman form while retaining Docker's required `sha256:` prefix; both forms
require exactly 64 hexadecimal characters. Engine-specific malformed-ID and
tag-protection tests pass. Cleanup ran; the sixth full workflow passed.
