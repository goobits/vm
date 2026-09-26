#!/usr/bin/env bash
# Disposable native Tart and broader CLI acceptance; requires a Linux guest agent image.
set -Eeuo pipefail
repo=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
source_binary=${VM_ACCEPTANCE_BIN:?Set VM_ACCEPTANCE_BIN to the current release binary}
tart_image=${VM_ACCEPTANCE_TART_IMAGE:-ghcr.io/cirruslabs/ubuntu:latest}
tart_binary=$(command -v tart)
root=$(mktemp -d "${TMPDIR:-/tmp}/vm-tart-acceptance.XXXXXX")
project=vm-tart-acceptance-$$
mkdir -p "$root/home" "$root/tart" "$root/project" "$root/other" "$root/bin"
cp "$source_binary" "$root/vm-under-test"
cp "$0" "$root/workflow.sh"
for excluded in docker podman; do
  printf '#!/bin/sh\nexit 127\n' > "$root/bin/$excluded"
  chmod +x "$root/bin/$excluded"
done
export PATH="$root/bin:$PATH"
v() { (cd "$root/project" && HOME="$root/home" TART_HOME="$root/tart" "$root/vm-under-test" "$@"); }
run_tart() { HOME="$root/home" TART_HOME="$root/tart" "$tart_binary" "$@"; }
fail() { if v "$@" > "$root/expected-failure.log" 2>&1; then echo "Unexpected success: $*"; return 1; fi; cat "$root/expected-failure.log"; }
cleanup() {
  status=$?
  trap - EXIT ERR
  set +e
  run_tart list --format json > "$root/final-tart.json"
  python3 - "$root/final-tart.json" > "$root/local-vms" <<'PYTHON'
import json, pathlib, sys
for entry in json.loads(pathlib.Path(sys.argv[1]).read_text()):
    if entry['Source'] == 'local': print(entry['Name'])
PYTHON
  while IFS= read -r name; do
    run_tart stop "$name" >/dev/null 2>&1
    run_tart delete "$name"
  done < "$root/local-vms"
  echo "Acceptance exit=$status; evidence preserved at $root"
  exit "$status"
}
trap cleanup EXIT
trap 'echo "FAIL line $LINENO"' ERR
exec > >(tee "$root/run.log") 2>&1
printf 'Evidence: %s\nProject: %s\n' "$root" "$project"
git -C "$repo" rev-parse HEAD
shasum -a 256 "$root/vm-under-test"
run_tart --version
uname -a
cat > "$root/project/vm.yaml" <<YAML
version: '2.0'
provider: tart
project:
  name: $project
  workspace_path: /workspace
vm:
  image: $tart_image
  user: admin
  cpus: 2
  memory: 2048
  gui: false
tart:
  guest_os: linux
  ssh_user: admin
  install_docker: false
  rosetta: false
  disk_size: 20
terminal:
  shell: bash
host_sync:
  git_config: false
  ai_tools: false
bootstrap:
  dependencies: false
YAML
v config validate
v create dev --provider tart --image "$tart_image"

v start dev
v --quiet exec --env dev -- sh -c 'printf snapshot-data > /home/admin/acceptance-marker; sync; printf stdout-bytes; printf stderr-bytes >&2' > "$root/exec.stdout" 2> "$root/exec.stderr"
python3 - "$root" <<'PY'
import pathlib,sys
p=pathlib.Path(sys.argv[1]); assert (p/'exec.stdout').read_bytes()==b'stdout-bytes'; assert (p/'exec.stderr').read_bytes()==b'stderr-bytes'
PY
fail snapshots create running --env dev
v stop dev
v snapshots create clean --env dev --json > "$root/create-snapshot.json"
fail snapshots create clean --env dev
for compression in gzip none; do
 v snapshots export clean --output "$root/$compression.archive" --compression "$compression" --json > "$root/export-$compression.json"
 fail snapshots export clean --output "$root/$compression.archive" --compression "$compression"
 v snapshots export clean --output "$root/$compression.archive" --compression "$compression" --overwrite
 v snapshots import "$root/$compression.archive" --name "imported-$compression" --json > "$root/import-$compression.json"
 fail snapshots import "$root/$compression.archive" --name "imported-$compression"
done
python3 - "$root" <<'PY'
import pathlib,sys,json,tarfile
p=pathlib.Path(sys.argv[1]); assert (p/'gzip.archive').open('rb').read(2)==b'\x1f\x8b'; assert (p/'none.archive').open('rb').read(2)!=b'\x1f\x8b'
for kind in ['gzip','none']:
 with tarfile.open(p/(kind+'.archive')) as t: assert any(m.name.endswith('manifest.json') for m in t)
for f in p.glob('*-*.json'): json.loads(f.read_text())
metadata=[x for x in (p/'home').rglob('metadata.json') if json.loads(x.read_text()).get('name')=='clean']; assert len(metadata)==1
m=metadata[0]; data=json.loads(m.read_text()); assert data['provider']=='tart' and data['native_image_digest'].startswith('sha256:')
(p/'snapshot-metadata-path').write_text(str(m)); (p/'clean-metadata.json').write_bytes(m.read_bytes())
data['native_image_digest']='sha256:'+'0'*64;m.write_text(json.dumps(data))
PY
fail snapshots restore clean --env dev --yes
python3 - "$root" <<'PY'
import pathlib,sys
p=pathlib.Path(sys.argv[1]);pathlib.Path((p/'snapshot-metadata-path').read_text()).write_bytes((p/'clean-metadata.json').read_bytes())
(p/'broken.archive').write_bytes((p/'gzip.archive').open('rb').read(4096))
PY
python3 - "$root" <<'PYTHON'
import pathlib,sys,json,hashlib
p=pathlib.Path(sys.argv[1]); m=pathlib.Path((p/'snapshot-metadata-path').read_text());d=json.loads(m.read_text());n=m.parent/'native'/d['native_vm_file'];n.rename(n.with_suffix('.valid'));n.write_bytes(b'invalid native Tart image');d['native_image_digest']='sha256:'+hashlib.sha256(n.read_bytes()).hexdigest();m.write_text(json.dumps(d))
PYTHON
fail snapshots restore clean --env dev --yes
python3 - "$root" <<'PYTHON'
import pathlib,sys,json
p=pathlib.Path(sys.argv[1]);m=pathlib.Path((p/'snapshot-metadata-path').read_text());d=json.loads((p/'clean-metadata.json').read_text());n=m.parent/'native'/d['native_vm_file'];n.with_suffix('.valid').replace(n);m.write_bytes((p/'clean-metadata.json').read_bytes())
PYTHON
v start dev
v --quiet exec --env dev -- sh -c 'test "$(cat /home/admin/acceptance-marker)" = snapshot-data'
v stop dev
fail snapshots import "$root/broken.archive" --name recovered
v snapshots import "$root/gzip.archive" --name recovered
for compression in gzip none; do
 v start dev
 v exec --env dev -- sh -c 'printf changed > /home/admin/acceptance-marker; touch /home/admin/after-snapshot; sync'
 v stop dev
 v snapshots restore "imported-$compression" --env dev --yes --json > "$root/restore-$compression.json"
 v start dev
 v --quiet exec --env dev -- sh -c 'test "$(cat /home/admin/acceptance-marker)" = snapshot-data && test ! -e /home/admin/after-snapshot'
 v stop dev
done
v snapshots restore clean --env dev --yes
v snapshots list --env dev --json > "$root/snapshots.json"
echo 'PASS Tart snapshot workflow'

cp "$root/project/vm.yaml" "$root/other/vm.yaml"
cat >> "$root/other/vm.yaml" <<'YAML'
services:
  postgresql:
    enabled: true
    database: acceptance
YAML
fail db list --env dev --config "$root/other/vm.yaml"
grep -q "'tart' is unsupported" "$root/expected-failure.log"
fail db backups create --database acceptance --env dev --config "$root/other/vm.yaml"
grep -q "'tart' is unsupported" "$root/expected-failure.log"
cp "$root/project/vm.yaml" "$root/other/vm.yaml"
cat >> "$root/other/vm.yaml" <<'YAML'
services:
  cache:
    enabled: true
    plugin: acceptance
YAML
fail config validate --config "$root/other/vm.yaml"
grep -q 'Service plugins require the Docker or Podman provider' "$root/expected-failure.log"

v create peer --provider tart --image "$tart_image"
v start peer
v exec --env peer -- sh -c 'printf peer-unchanged > /home/admin/peer-marker; sync'
v stop peer
fail snapshots restore clean --env peer --yes
grep -q 'was not captured from environment' "$root/expected-failure.log"
v start --all-envs
v --quiet exec --env peer -- sh -c 'test "$(cat /home/admin/peer-marker)" = peer-unchanged'
cp "$root/project/vm.yaml" "$root/other/vm.yaml"
fail snapshots show clean --config "$root/other/vm.yaml"
v snapshots import "$root/gzip.archive" --name foreign --config "$root/other/vm.yaml"
fail snapshots restore foreign --env dev --yes --config "$root/other/vm.yaml"
grep -q 'No environment matches' "$root/expected-failure.log"
v --quiet exec --env dev -- sh -c 'test "$(cat /home/admin/acceptance-marker)" = snapshot-data'

v --quiet exec --env dev --user root -- id -u | grep '^0$'
status=0
v --quiet exec --env dev -- sh -c 'exit 17' || status=$?
test "$status" = 17
fail shell dev --cwd /tmp
v exec --env dev --cwd /tmp -- pwd | grep '^/tmp$'
copy_target="/tmp/$project-copied"
printf 'copy-bytes\n' > "$root/copy-source"
v copy --env dev "host:$root/copy-source" "env:$copy_target"
fail copy --env dev "host:$root/copy-source" "env:$copy_target"
v copy --env dev "host:$root/copy-source" "env:$copy_target" --overwrite
v copy --env dev "env:$copy_target" "host:$root/copy-destination"
cmp "$root/copy-source" "$root/copy-destination"
v exec --env dev --env peer --output json-lines -- printf 'application bytes\n' > "$root/fleet.jsonl"
v exec --env dev --env peer --output grouped -- printf 'grouped bytes\n'
status=0
v --quiet exec --env dev --env peer --output json-lines -- sh -c 'exit 17' > "$root/fleet-failure.jsonl" || status=$?
test "$status" = 1
v --quiet exec --env dev -- printf 'application-log-bytes\n' > "$root/tart/vms/$project-dev/app.log"
v logs dev --tail 5 --json-lines > "$root/logs.jsonl"
v --quiet logs dev --tail 1 > "$root/human-logs.bin"
printf 'application-log-bytes\n' > "$root/expected-human-logs.bin"
cmp "$root/human-logs.bin" "$root/expected-human-logs.bin"
fail logs dev --service cache
grep -q 'Tart logs do not support --service' "$root/expected-failure.log"
fail tunnels open unsupported --env dev --local 127.0.0.1:49371 --remote localhost:8000
grep -q "'tart' is not supported" "$root/expected-failure.log"
python3 - "$root" <<'PY'
import base64,json,pathlib,sys
p=pathlib.Path(sys.argv[1]);events=[json.loads(x) for x in (p/'fleet.jsonl').read_text().splitlines()];assert events[-1]['ok'] and events[-1]['targets']==2
assert [base64.b64decode(x['data_base64']) for x in events if x.get('stream')=='stdout']==[b'application bytes\n']*2
failures=[json.loads(x) for x in (p/'fleet-failure.jsonl').read_text().splitlines()];assert [x['exit_code'] for x in failures if x['type']=='target_result']==[17,17];assert failures[-1]['failed']==2 and failures[-1]['ok'] is False
logs=[json.loads(x) for x in (p/'logs.jsonl').read_text().splitlines()];assert logs[-1]['ok'];assert any(b'application-log-bytes' in base64.b64decode(x['data_base64']) for x in logs[:-1])
PY
v status --all-envs --json > "$root/status.json"
v doctor dev
v restart dev
v stop --all-envs
v start --all-envs
v stop dev peer
fail exec --env dev -- true
v system storage list --json > "$root/storage.json"
fail system storage remove "tart:vm:$project-dev" --yes
grep -q 'registered project configuration still exists' "$root/expected-failure.log"
python3 - "$root" "$project" <<'PYTHON'
import json,pathlib,sys
p=pathlib.Path(sys.argv[1]); resources=json.loads((p/'storage.json').read_text())['data'];assert {x['id'] for x in resources if x['id'].startswith('tart:vm:')}=={f'tart:vm:{sys.argv[2]}-dev',f'tart:vm:{sys.argv[2]}-peer'}
PYTHON
mv "$root/project/vm.yaml" "$root/project/vm.yaml.saved"
v system storage list --json > "$root/unreferenced-storage.json"
v system storage remove "tart:vm:$project-peer" --yes
v system storage remove "tart:vm:$project-dev" --yes
mv "$root/project/vm.yaml.saved" "$root/project/vm.yaml"
echo 'PASS Tart broader workflow'
date -u '+Completed %Y-%m-%dT%H:%M:%SZ'
