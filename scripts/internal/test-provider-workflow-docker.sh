#!/usr/bin/env bash
# Disposable live CLI acceptance. Logs and project state are retained on failure.
# Docker is the default; set VM_ACCEPTANCE_PROVIDER=podman for an isolated Podman connection.
set -Eeuo pipefail
repo=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
vm_binary=${VM_ACCEPTANCE_BIN:?Set VM_ACCEPTANCE_BIN to the current release binary}
docker_config=${DOCKER_CONFIG:-$HOME/.docker}
provider=${VM_ACCEPTANCE_PROVIDER:-docker}
case "$provider" in docker|podman) ;; *) echo 'VM_ACCEPTANCE_PROVIDER must be docker or podman' >&2; exit 2 ;; esac
engine() { command "$provider" "$@"; }
root=$(mktemp -d "${TMPDIR:-/tmp}/vm-provider-acceptance.XXXXXX")
project=vm-provider-acceptance-$$
mkdir -p "$root/home" "$root/project" "$root/other" "$root/plugin"
cp "$vm_binary" "$root/vm-under-test"
vm_binary=$root/vm-under-test
# Keep the tested binary stable across rebuilds and exclude unrelated providers
# from inventory while preserving the selected engine's real connection.
mkdir -p "$root/bin" "$root/tart"
for excluded_provider in docker podman tart; do
  if test "$excluded_provider" != "$provider"; then
    printf '#!/bin/sh\nexit 127\n' > "$root/bin/$excluded_provider"
    chmod +x "$root/bin/$excluded_provider"
  fi
done
export PATH="$root/bin:$PATH" TART_HOME="$root/tart"
cp "$0" "$root/workflow.sh"
exec > >(tee "$root/run.log") 2>&1
printf 'Evidence: %s\nProject: %s\n' "$root" "$project"
run_vm() { (cd "$root/project" && HOME="$root/home" XDG_CONFIG_HOME="$root/home/.config" XDG_DATA_HOME="$root/home/.local/share" XDG_CACHE_HOME="$root/home/.cache" DOCKER_CONFIG="$docker_config" "$vm_binary" "$@"); }
fail_vm() { if run_vm "$@" > "$root/expected-failure.log" 2>&1; then echo "Unexpected success: $*"; return 1; fi; cat "$root/expected-failure.log"; }
cleanup() {
  local status=$? resource
  trap - EXIT ERR
  set +e
  mkdir -p "$root/container-evidence"
  for resource in $(engine ps -aq --filter "label=com.vm.project=$project"); do
    engine inspect "$resource" > "$root/container-evidence/$resource.json"
    engine logs "$resource" > "$root/container-evidence/$resource.log" 2>&1
  done
  engine ps -a --filter "label=com.vm.project=$project" > "$root/final-containers.txt"
  engine volume ls --filter "label=com.vm.project=$project" > "$root/final-volumes.txt"
  run_vm tunnels close relay --env dev > /dev/null 2>&1
  for resource in $(engine ps -aq --filter "label=com.vm.project=$project"); do engine rm -f "$resource"; done
  for resource in $(engine volume ls -q --filter "label=com.vm.project=$project"); do engine volume rm "$resource"; done
  for resource in $(engine network ls -q --filter "label=com.docker.compose.project=$project-dev") $(engine network ls -q --filter "label=com.docker.compose.project=$project-peer"); do engine network rm "$resource"; done
  for resource in $(engine image ls -q --filter "label=com.vm.project=$project" | sort -u); do engine image rm "$resource"; done
  if test -f "$root/environment-images"; then
    while IFS= read -r resource; do
      if ! grep -Fxq "$resource" "$root/existing-images"; then engine image rm "$resource"; fi
    done < "$root/environment-images"
  fi
  engine image rm "$project-cache:acceptance" "vm-custom-$project:latest"
  echo "Acceptance exit=$status; evidence preserved at $root"
  exit "$status"
}
trap cleanup EXIT
trap 'echo "Failure at line $LINENO"' ERR
trap 'exit 130' INT TERM HUP
git -C "$repo" rev-parse HEAD
python3 - "$vm_binary" <<'PYTHON'
import hashlib, pathlib, sys
path = pathlib.Path(sys.argv[1])
print('Binary SHA256:', hashlib.sha256(path.read_bytes()).hexdigest())
PYTHON
engine image ls --no-trunc --format '{{.ID}}' > "$root/existing-images"
: > "$root/environment-images"
engine version
engine compose version
"$vm_binary" --version
db_port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
relay_port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
cat > "$root/plugin/plugin.yaml" <<YAML
name: acceptance-cache
version: 1.0.0
plugin_type: service
YAML
# The restricted plugin runs without capabilities, so bypass Redis's root
# entrypoint user switch in this disposable fixture image.
printf 'FROM docker.io/library/redis:7-alpine\nENTRYPOINT ["redis-server"]\n' > "$root/plugin/Dockerfile"
engine build --tag "$project-cache:acceptance" "$root/plugin"
printf 'image: %s-cache:acceptance\nvolumes: ["cache_data:/data"]\n' "$project" > "$root/plugin/service.yaml"
mv "$root/plugin/Dockerfile" "$root/Plugin.Dockerfile"
run_vm plugins validate "$root/plugin"
run_vm plugins install "$root/plugin"
cp "$repo/scripts/internal/package-workflow-docker/fixtures/environment.Dockerfile" "$root/project/Dockerfile.acceptance"
# Fully qualify the base for Podman's noninteractive short-name resolution.
sed 's/^FROM node:/FROM docker.io\/library\/node:/' "$root/project/Dockerfile.acceptance" > "$root/project/Dockerfile.qualified"
mv "$root/project/Dockerfile.qualified" "$root/project/Dockerfile.acceptance"
cat > "$root/project/vm.yaml" <<YAML
version: '2.0'
provider: $provider
project:
  name: $project
  workspace_path: /workspace
vm:
  user: acceptance
  uid: 11000
  gid: 11000
  image:
    dockerfile: Dockerfile.acceptance
    context: .
terminal:
  shell: bash
host_sync:
  git_config: false
  ai_tools: false
bootstrap:
  dependencies: false
storage:
  volumes:
    acceptance:
      target: /acceptance-data
      scope: instance
      retention: disposable
      nocopy: true
environments:
  dev:
    provider: $provider
    image:
      dockerfile: Dockerfile.acceptance
      context: .
    services:
      postgresql:
        enabled: true
        database: acceptance
        user: postgres
        password: acceptance-only
        port: $db_port
      cache:
        enabled: true
        plugin: acceptance-cache
  peer:
    provider: $provider
    image:
      dockerfile: Dockerfile.acceptance
      context: .
YAML
run_vm config validate
run_vm start dev
engine inspect --format '{{.Image}}' "$project-dev-dev" >> "$root/environment-images"
run_vm exec --env dev --user root -- sh -c 'printf restored > /acceptance-data/marker; printf rootfs > /root/acceptance-marker'
run_vm --quiet exec --env dev -- sh -c 'printf stdout-bytes; printf stderr-bytes >&2' > "$root/exec.stdout" 2> "$root/exec.stderr"
test "$(cat "$root/exec.stdout")" = stdout-bytes
test "$(cat "$root/exec.stderr")" = stderr-bytes
run_vm exec --env dev --cwd /tmp -- pwd | grep '^/tmp$'
fail_vm shell dev --cwd /tmp
printf 'copy-bytes\n' > "$root/source"
run_vm copy --env dev "host:$root/source" env:/tmp/copied
fail_vm copy --env dev "host:$root/source" env:/tmp/copied
printf 'replacement-copy-bytes\n' > "$root/source"
run_vm copy --env dev --overwrite "host:$root/source" env:/tmp/copied
run_vm copy --env dev env:/tmp/copied "host:$root/copied"
cmp "$root/source" "$root/copied"
run_vm snapshots create clean --env dev --quiesce
run_vm exec --env dev --user root -- cat /acceptance-data/marker | grep '^restored$'
run_vm snapshots show clean --json > "$root/snapshot.json"
fail_vm snapshots create clean --env dev
for compression in gzip none; do
  run_vm snapshots export clean --output "$root/$compression.archive" --compression "$compression"
  fail_vm snapshots export clean --output "$root/$compression.archive" --compression "$compression"
  run_vm snapshots export clean --output "$root/$compression.archive" --compression "$compression" --overwrite
  run_vm snapshots import "$root/$compression.archive" --name "imported-$compression"
  fail_vm snapshots import "$root/$compression.archive" --name "imported-$compression"
done
python3 - "$root" <<'PYTHON'
import json, pathlib, tarfile, sys
root = pathlib.Path(sys.argv[1])
formats = {}
for compression in ('gzip', 'none'):
    path = root / (compression + '.archive')
    with path.open('rb') as source:
        assert (source.read(2) == b'\x1f\x8b') == (compression == 'gzip')
    with tarfile.open(path, 'r:gz' if compression == 'gzip' else 'r:') as archive:
        assert any(name.endswith('manifest.json') for name in archive.getnames())
    formats[compression] = {'bytes': path.stat().st_size, 'verified': True}
(root / 'archive-formats.json').write_text(json.dumps(formats, indent=2) + '\n')
PYTHON
# Missing captured image: restore must reject before changing live data.
metadata=$(find "$root/home" -path '*/clean/metadata.json' -type f)
test -n "$metadata"
snapshot_image=$(python3 - "$metadata" <<'PYTHON'
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
data = json.loads(path.read_text())
assert data['services'], 'Snapshot omitted all container root filesystems'
print(path.parent / 'images' / data['services'][0]['image_file'])
PYTHON
)
mv "$snapshot_image" "$snapshot_image.saved"
fail_vm snapshots restore clean --env dev --yes
run_vm exec --env dev --user root -- cat /acceptance-data/marker | grep '^restored$'
mv "$snapshot_image.saved" "$snapshot_image"
cp "$metadata" "$root/clean-metadata.json"
# An imported/incomplete container snapshot must fail before replacing its target.
before_empty_restore=$(engine inspect --format '{{.Id}}' "$project-dev-dev")
python3 - "$metadata" <<'PYTHON'
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
data = json.loads(path.read_text())
data['services'] = []
path.write_text(json.dumps(data))
PYTHON
fail_vm snapshots restore clean --env dev --yes
grep -q 'no captured service images' "$root/expected-failure.log"
test "$(engine inspect --format '{{.Id}}' "$project-dev-dev")" = "$before_empty_restore"
run_vm exec --env dev --user root -- cat /acceptance-data/marker | grep '^restored$'
cp "$root/clean-metadata.json" "$metadata"
python3 - "$metadata" <<'PYTHON'
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
data = json.loads(path.read_text())
data['services'][0]['image_digest'] = 'sha256:' + '0' * 64
path.write_text(json.dumps(data))
PYTHON
fail_vm snapshots restore clean --env dev --yes
run_vm exec --env dev --user root -- cat /acceptance-data/marker | grep '^restored$'
cp "$root/clean-metadata.json" "$metadata"
snapshot_volume=$(python3 - "$metadata" <<'PYTHON'
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
print(path.parent / 'volumes' / json.loads(path.read_text())['volumes'][0]['archive_file'])
PYTHON
)
cp "$snapshot_volume" "$snapshot_volume.saved"
printf corrupt > "$snapshot_volume"
fail_vm snapshots restore clean --env dev --yes
run_vm exec --env dev --user root -- cat /acceptance-data/marker | grep '^restored$'
mv "$snapshot_volume.saved" "$snapshot_volume"
run_vm snapshots restore clean --env dev --yes
for compression in gzip none; do
  run_vm exec --env dev --user root -- sh -c 'printf changed > /acceptance-data/marker; touch /acceptance-data/after-snapshot; printf changed > /root/acceptance-marker'
  run_vm snapshots restore "imported-$compression" --env dev --yes --json > "$root/restore-$compression.json"
  python3 - "$root/restore-$compression.json" <<'PYTHON'
import json, sys
with open(sys.argv[1]) as source:
    result = json.load(source)
assert result['ok'] and result['command'] == 'snapshots restore'
PYTHON
  run_vm exec --env dev --user root -- cat /acceptance-data/marker | grep '^restored$'
  run_vm exec --env dev --user root -- cat /root/acceptance-marker | grep '^rootfs$'
  run_vm exec --env dev --user root -- test ! -e /acceptance-data/after-snapshot
done
# An archive with modified payload must fail validation without registering a name.
python3 - "$root" <<'PYTHON'
import io, pathlib, sys, tarfile
root = pathlib.Path(sys.argv[1])
with tarfile.open(root/'none.archive') as src, tarfile.open(root/'corrupt.tar', 'w') as dst:
    for member in src:
        payload = src.extractfile(member).read() if member.isfile() else None
        if member.name.endswith('metadata.json'):
            payload += b' '
            member.size = len(payload)
        dst.addfile(member, io.BytesIO(payload) if payload is not None else None)
PYTHON
fail_vm snapshots import "$root/corrupt.tar" --name recovery
run_vm snapshots import "$root/gzip.archive" --name recovery
cp "$root/project/vm.yaml" "$root/other/vm.yaml"
if (cd "$root/other" && HOME="$root/home" XDG_CONFIG_HOME="$root/home/.config" XDG_DATA_HOME="$root/home/.local/share" XDG_CACHE_HOME="$root/home/.cache" DOCKER_CONFIG="$docker_config" "$vm_binary" snapshots show clean); then
  echo 'Cross-project snapshot lookup unexpectedly succeeded'; exit 1
fi
pg=$(engine ps -q --filter "label=com.docker.compose.project=$project-dev" --filter label=com.docker.compose.service=postgres)
test -n "$pg"
for attempt in $(seq 1 30); do engine exec "$pg" pg_isready -U postgres && break; sleep 1; done
engine exec "$pg" psql -U postgres -d acceptance -c "CREATE TABLE marker(value text); INSERT INTO marker VALUES ('database-restored');"
engine exec "$pg" createdb -U postgres second
run_vm db list --env dev > "$root/databases.json"
run_vm db credentials postgresql --env dev > "$root/credentials.json"
if grep -q acceptance-only "$root/credentials.json"; then echo 'Credential leaked'; exit 1; fi
run_vm db backups create acceptance-backup --all --env dev
run_vm db backups list --env dev > "$root/backups.json"
run_vm db export acceptance --env dev --output "$root/database.sql"
fail_vm db export acceptance --env dev --output "$root/database.sql"
engine exec "$pg" psql -U postgres -d acceptance -c 'DELETE FROM marker;'
backup=$(run_vm db backups list --database acceptance --env dev | grep "^acceptance_.*[.]dump$")
test -n "$backup"
run_vm db backups restore "$backup" --database acceptance --env dev --yes
engine exec "$pg" psql -U postgres -d acceptance -Atc 'SELECT value FROM marker' | grep '^database-restored$'
run_vm db backups remove "$backup" --env dev --yes
cache=$(engine ps -q --filter "label=com.docker.compose.project=$project-dev" --filter label=com.docker.compose.service=plugin_cache)
engine exec "$cache" redis-cli CONFIG SET protected-mode no
run_vm tunnels open relay --local "127.0.0.1:$relay_port" --remote cache:6379 --env dev
run_vm tunnels open relay --local "127.0.0.1:$relay_port" --remote cache:6379 --env dev
python3 - "$relay_port" <<'PYTHON'
import socket, sys
with socket.create_connection(('127.0.0.1', int(sys.argv[1])), timeout=10) as conn:
    conn.sendall(b'*1\r\n$4\r\nPING\r\n')
    response = conn.recv(4096)
    assert response == b'+PONG\r\n', repr(response)
PYTHON
run_vm tunnels list --json > "$root/tunnels.json"
fail_vm tunnels open relay --local "127.0.0.1:$relay_port" --remote cache:6380 --env dev
fail_vm tunnels open ipv6 --local "[::1]:$relay_port" --remote cache:6379 --env dev
run_vm start peer
engine inspect --format '{{.Image}}' "$project-peer-dev" >> "$root/environment-images"
run_vm exec --env peer --user root -- sh -c 'printf peer-unchanged > /acceptance-data/marker'
fail_vm snapshots restore clean --env peer --yes
grep -q "was not captured from environment" "$root/expected-failure.log"
run_vm exec --env peer --user root -- cat /acceptance-data/marker | grep '^peer-unchanged$'
fail_vm db list --env peer
run_vm exec --env dev --env peer --output json-lines -- printf 'application bytes\n' > "$root/fleet.jsonl"
run_vm exec --env dev --env peer --output grouped -- printf 'grouped bytes\n'
fail_vm exec --env dev --env peer --output json-lines -- sh -c 'exit 17'
run_vm exec --env dev --user root -- sh -c 'printf "application-log-bytes\n" > /proc/1/fd/1'
run_vm logs dev --tail 5 --json-lines > "$root/logs.jsonl"
python3 - "$root" <<'PYTHON'
import base64, json, pathlib, sys
root = pathlib.Path(sys.argv[1])
fleet = [json.loads(line) for line in (root/'fleet.jsonl').read_text().splitlines()]
assert fleet[-1]['ok'] and fleet[-1]['targets'] == 2
outputs = [base64.b64decode(event['data_base64']) for event in fleet if event.get('stream') == 'stdout']
assert outputs == [b'application bytes\n', b'application bytes\n']
logs = [json.loads(line) for line in (root/'logs.jsonl').read_text().splitlines()]
assert logs[-1]['ok'] and logs[-1]['records'] > 0
assert any(b'application-log-bytes' in base64.b64decode(event['data_base64']) for event in logs[:-1])
PYTHON
run_vm status --all-envs --json > "$root/status.json"
run_vm doctor dev
run_vm restart dev
run_vm stop --all-envs
run_vm start --all-envs
run_vm stop dev peer
fail_vm exec --env dev -- true
run_vm system storage list --json > "$root/storage.json"
# Retained instance data is visible and removable only after its runtime is gone.
volume=vm_${project}-peer_acceptance
fail_vm system storage remove "$provider:volume:$volume" --yes
run_vm remove peer --yes
run_vm system storage list --json > "$root/retained-storage.json"
fail_vm system storage remove "$provider:volume:vm_${project}-peer_shell_history" --yes
grep -q "$volume" "$root/retained-storage.json"
run_vm system storage remove "$provider:volume:$volume" --yes
if engine volume inspect "$volume" > /dev/null 2>&1; then echo 'Retained volume was not removed'; exit 1; fi
run_vm remove dev --delete-data --yes
run_vm tunnels list --env dev --json > "$root/orphan-tunnels.json"
grep -q relay "$root/orphan-tunnels.json"
run_vm tunnels close relay --env dev
run_vm snapshots remove recovery --yes
run_vm snapshots remove clean --yes
run_vm snapshots remove imported-gzip --yes
run_vm snapshots remove imported-none --yes
run_vm plugins remove acceptance-cache
echo "PASS $provider provider workflow"
