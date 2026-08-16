#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
vm_binary=${VM_ACCEPTANCE_BIN:-$repository_root/rust/target/release/vm}
project_name=package-producer-acceptance
environment_name=$project_name-dev
edge_name=$project_name-package-edge
consumer_name=package-consumer-acceptance
consumer_environment=$consumer_name-dev
consumer_edge=$consumer_name-package-edge
acceptance_root=$(mktemp -d "${TMPDIR:-/tmp}/vm-package-acceptance.XXXXXX")
acceptance_home=$acceptance_root/home
source_shelf=$acceptance_root/sources
project_root=$source_shelf/release-tool
consumer_root=$acceptance_root/consumer
fixture_root=$acceptance_root/agent-skills
fake_bin=$acceptance_root/bin
work_log=$acceptance_root/work.log
workspace_log=$acceptance_root/workspace.log
before_ids=$acceptance_root/before.ids
after_ids=$acceptance_root/after.ids
before_volumes=$acceptance_root/before.volumes
after_volumes=$acceptance_root/after.volumes
work_pid=

run_vm() {
  env HOME="$acceptance_home" PATH="$fake_bin:$(dirname "$vm_binary"):$PATH" "$vm_binary" "$@"
}

cleanup() {
  set +e
  if test -n "$work_pid" && kill -0 "$work_pid" 2>/dev/null; then
    kill "$work_pid" 2>/dev/null
    wait "$work_pid" 2>/dev/null
  fi
  run_vm --config "$project_root/vm.yaml" destroy --force --all >/dev/null 2>&1
  run_vm --config "$consumer_root/vm.yaml" destroy --force --all >/dev/null 2>&1
  docker rm --force "$environment_name" "$edge_name" \
    "$consumer_environment" "$consumer_edge" >/dev/null 2>&1
  package_compose=$acceptance_home/.vm/infrastructure/packages/compose.yaml
  package_environment=$acceptance_home/.vm/infrastructure/packages/environment.env
  if test -f "$package_compose" && test -f "$package_environment"; then
    docker compose --project-name vm-packages --file "$package_compose" \
      --env-file "$package_environment" down --volumes --remove-orphans >/dev/null 2>&1
  fi
  case "$acceptance_root" in
    */vm-package-acceptance.*) rm -rf -- "$acceptance_root" ;;
  esac
}
trap cleanup EXIT

capture_runtime_state() {
  local ids=$1
  local volumes=$2
  local volume_names=$acceptance_root/volume-names
  : > "$volume_names"
  for container in "${stable_containers[@]}"; do
    docker inspect --format '{{.Name}} {{.Id}}' "$container"
    docker inspect --format '{{range .Mounts}}{{if eq .Type "volume"}}{{println .Name}}{{end}}{{end}}' \
      "$container" >> "$volume_names"
  done | sort > "$ids"
  : > "$volumes"
  sort -u "$volume_names" | while IFS= read -r volume; do
    test -z "$volume" || docker volume inspect \
      --format '{{.Name}} {{.CreatedAt}} {{.Mountpoint}}' "$volume"
  done | sort > "$volumes"
}

test -x "$vm_binary" || {
  echo "Acceptance VM binary is missing: $vm_binary" >&2
  echo "Repair: cargo build --manifest-path rust/Cargo.toml --release --package goobits-vm" >&2
  exit 2
}
command -v docker >/dev/null 2>&1 || {
  echo "Docker is unavailable" >&2
  echo "Repair: install Docker Engine and rerun this script" >&2
  exit 2
}

mkdir -p "$acceptance_home" "$project_root" "$consumer_root" \
  "$fixture_root/skills/initial" "$fake_bin"

version=$("$vm_binary" --version | awk '{print $2}')
server_image=ghcr.io/goobits/vm-package-server:$version
jobs_image=ghcr.io/goobits/vm-package-jobs:$version
docker build --provenance=false --tag "$server_image" \
  --file "$repository_root/rust/vm-package-server/docker/server/Dockerfile" "$repository_root"
docker build --provenance=false --tag "$jobs_image" \
  --file "$repository_root/rust/vm-package-jobs/Dockerfile" "$repository_root"

cat > "$fake_bin/gh" <<'GH'
#!/bin/sh
case "$*" in
  "auth status --hostname github.com") exit 0 ;;
  "auth token --hostname github.com") printf '%s\n' acceptance-token-not-used-for-local-git ;;
  *) exit 2 ;;
esac
GH
chmod 0755 "$fake_bin/gh"

cat > "$project_root/Dockerfile.acceptance" <<'DOCKERFILE'
FROM ubuntu:24.04
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl git python3 sudo bash tar gzip && \
    rm -rf /var/lib/apt/lists/* && \
    groupadd --gid 11000 acceptance && \
    useradd --create-home --uid 11000 --gid 11000 --shell /bin/bash acceptance && \
    printf '%s\n' 'acceptance ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/acceptance && \
    chmod 0440 /etc/sudoers.d/acceptance && \
    install -d -o acceptance -g acceptance /workspace && \
    sudo -Hu acceptance git config --global user.name 'VM Acceptance' && \
    sudo -Hu acceptance git config --global user.email 'vm-acceptance@example.invalid'
COPY codex /usr/local/lib/vm-ai-tools/codex-package/bin/codex
RUN printf '%s\n' '{}' > /usr/local/lib/vm-ai-tools/codex-package/codex-package.json && \
    printf '%s\n' '#!/bin/sh' 'exit 0' > /usr/local/lib/vm-ai-tools/codex-package/bin/codex-code-mode-host && \
    chmod 0755 /usr/local/lib/vm-ai-tools/codex-package/bin/* && \
    ln -s /usr/local/lib/vm-ai-tools/codex-package/bin/codex /usr/local/bin/codex && \
    ln -s /usr/local/lib/vm-ai-tools/codex-package/bin/codex-code-mode-host /usr/local/bin/codex-code-mode-host
USER acceptance
WORKDIR /workspace
CMD ["tail", "-f", "/dev/null"]
DOCKERFILE

cat > "$project_root/codex" <<'CODEX'
#!/bin/sh
set -eu
if test "${1:-}" = --version; then
  printf '%s\n' 'codex-acceptance 1.0.0'
  exit 0
fi
state="$HOME/.local/state/vm"
mkdir -p "$state"
hostname > "$state/package-acceptance-container"
touch "$state/package-acceptance-ready"
sleep 5

mkdir -p skills/acceptance
printf '%s\n' '# Acceptance skill' > skills/acceptance/SKILL.md
git add skills/acceptance/SKILL.md
git commit -m 'feat: add acceptance skill'

if vm packages release; then
  echo 'Initial release unexpectedly succeeded without a version bump' >&2
  exit 41
fi
touch "$state/package-acceptance-rework"
python3 -c 'import json; p="package.json"; d=json.load(open(p)); d["version"]="1.0.1"; open(p,"w").write(json.dumps(d, indent=2)+"\n")'
git add package.json
git commit -m 'fix: apply requested version bump'
vm packages release
CODEX
chmod 0755 "$project_root/codex"

cat > "$project_root/vm.yaml" <<'YAML'
version: '2.0'
provider: docker
project:
  name: package-producer-acceptance
  workspace_path: /workspace
vm:
  user: acceptance
  uid: 11000
  gid: 11000
  box:
    dockerfile: Dockerfile.acceptance
    context: .
terminal:
  shell: bash
host_sync:
  git_config: false
  ai_tools: false
bootstrap:
  dependencies: false
tools:
  updates: auto
  agent-skills: {}
  release-tool: {}
YAML

cp "$project_root/Dockerfile.acceptance" "$consumer_root/Dockerfile.acceptance"
cp "$project_root/codex" "$consumer_root/codex"
cat > "$consumer_root/vm.yaml" <<'YAML'
version: '2.0'
provider: docker
project:
  name: package-consumer-acceptance
  workspace_path: /workspace
vm:
  user: acceptance
  uid: 11000
  gid: 11000
  box:
    dockerfile: Dockerfile.acceptance
    context: .
terminal:
  shell: bash
host_sync:
  git_config: false
  ai_tools: false
bootstrap:
  dependencies: false
tools:
  updates: auto
  release-tool: {}
YAML

cat > "$project_root/vm-tool.yaml" <<'YAML'
schema: 1
kind: binary
version: 1.0.0
builds:
  - target: linux-amd64
    command: ["node", "build.mjs", "linux-amd64"]
    archive: dist/release-tool-linux-amd64.tar.gz
    links:
      .local/bin/release-tool: bin/release-tool
    verify: ["bin/release-tool", "--version"]
  - target: linux-arm64
    command: ["node", "build.mjs", "linux-arm64"]
    archive: dist/release-tool-linux-arm64.tar.gz
    links:
      .local/bin/release-tool: bin/release-tool
YAML
cat > "$project_root/build.mjs" <<'JAVASCRIPT'
import { chmodSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';

const target = process.argv[2];
if (!['linux-amd64', 'linux-arm64'].includes(target)) process.exit(2);
const manifest = readFileSync('vm-tool.yaml', 'utf8');
const version = manifest.match(/^version: ([^\s]+)$/m)?.[1];
if (!version) process.exit(3);
const stage = `dist/stage-${target}`;
const binary = `${stage}/bin/release-tool`;
rmSync(stage, { recursive: true, force: true });
mkdirSync(`${stage}/bin`, { recursive: true });
writeFileSync(binary, `#!/bin/sh\nprintf '%s\\n' '${version}'\n`);
chmodSync(binary, 0o755);
const archive = `dist/release-tool-${target}.tar.gz`;
const result = spawnSync('tar', ['-czf', archive, '-C', stage, 'bin'], { stdio: 'inherit' });
process.exit(result.status ?? 1);
JAVASCRIPT
printf '%s\n' '# Generic binary workspace acceptance fixture' > "$project_root/README.md"
git -C "$project_root" init --initial-branch main
git -C "$project_root" config user.name 'VM Acceptance'
git -C "$project_root" config user.email 'vm-acceptance@example.invalid'
git -C "$project_root" remote add origin https://127.0.0.1:1/release-tool.git
git -C "$project_root" add .
git -C "$project_root" commit -m 'feat: initial binary tool'

cat > "$fixture_root/package.json" <<'JSON'
{
  "name": "agent-skills",
  "version": "1.0.0",
  "scripts": {
    "test": "test -f skills/initial/SKILL.md"
  }
}
JSON
printf '%s\n' 'kind: collection' > "$fixture_root/vm-tool.yaml"
printf '%s\n' '# Initial skill' > "$fixture_root/skills/initial/SKILL.md"
git -C "$fixture_root" init --initial-branch main
git -C "$fixture_root" config user.name 'VM Acceptance'
git -C "$fixture_root" config user.email 'vm-acceptance@example.invalid'
git -C "$fixture_root" add .
git -C "$fixture_root" commit -m 'feat: initial collection'

(
  cd "$project_root"
  run_vm packages init "$source_shelf"
)
test "$(run_vm packages status)" = 'Package infrastructure: healthy'

run_vm --config "$consumer_root/vm.yaml" up

docker run --rm --user 0:0 \
  --volume vm-packages_source-mirrors:/data/sources \
  --volume "$fixture_root:/fixture:ro" \
  --entrypoint /bin/sh "$server_image" -ec \
  'git clone --bare /fixture /data/sources/acceptance-agent-skills.git && chown -R 10001:10001 /data/sources/acceptance-agent-skills.git'
run_vm tools register agent-skills --kind collection \
  --repository file:///data/sources/acceptance-agent-skills.git

(
  cd "$project_root"
  run_vm packages work agent-skills 'prove edit rework publish activation'
) >"$work_log" 2>&1 &
work_pid=$!

ready=false
for _ in $(seq 1 300); do
  if docker exec "$environment_name" \
    test -f /home/acceptance/.local/state/vm/package-acceptance-ready 2>/dev/null; then
    ready=true
    break
  fi
  sleep 2
done
test "$ready" = true || {
  cat "$work_log" >&2
  echo "Managed Codex session did not start" >&2
  echo "Repair: rerun this acceptance script on a healthy Docker host" >&2
  exit 3
}

stable_containers=(
  vm-packages-gateway-1
  vm-packages-oci-cache-1
  vm-packages-registry-1
  vm-packages-work-1
  vm-packages-reviewer-1
  vm-packages-releaser-1
  vm-packages-rollout-1
  "$environment_name"
  "$edge_name"
  "$consumer_environment"
  "$consumer_edge"
)
capture_runtime_state "$before_ids" "$before_volumes"

set +e
wait "$work_pid"
work_status=$?
work_pid=
set -e
if test "$work_status" -ne 0; then
  cat "$work_log" >&2
  echo "Managed package work failed" >&2
  echo "Repair: rerun this acceptance script and inspect the package worker logs" >&2
  exit "$work_status"
fi

docker exec "$environment_name" \
  test -f /home/acceptance/.local/state/vm/package-acceptance-rework
docker exec "$environment_name" \
  test -L /home/acceptance/.codex/skills/acceptance
run_vm tools show agent-skills | grep -F '1.0.1' >/dev/null
docker run --rm --user 10001:10001 \
  --volume vm-packages_source-mirrors:/data/sources:ro \
  --entrypoint git "$server_image" \
  --git-dir=/data/sources/acceptance-agent-skills.git show main:package.json | \
  grep -F '"version": "1.0.1"' >/dev/null

if ! docker exec --user acceptance "$environment_name" sh -ec '
  cd /workspace
  test -z "$(git status --porcelain --untracked-files=all)"
  printf "%s\n" "workspace release change" >> README.md
  git add README.md
  git commit -m "feat: update binary behavior"
  unchanged=$(git rev-parse HEAD)
  if vm packages release; then
    echo "Initial workspace release unexpectedly succeeded without a version bump" >&2
    exit 42
  fi
  test "$(git rev-parse HEAD)" = "$unchanged"
  test -z "$(git status --porcelain --untracked-files=all)"
  test -z "$(git tag --list)"
  sed -i "s/version: 1.0.0/version: 1.0.1/" vm-tool.yaml
  git add vm-tool.yaml
  git commit -m "fix: apply requested binary version bump"
  released=$(git rev-parse HEAD)
  vm packages release
  test "$(git rev-parse HEAD)" = "$released"
  test -z "$(git status --porcelain --untracked-files=all)"
  test -z "$(git tag --list)"
  test "$(git remote get-url origin)" = "https://127.0.0.1:1/release-tool.git"
' >"$workspace_log" 2>&1; then
  cat "$workspace_log" >&2
  echo "Canonical workspace release failed" >&2
  echo "Repair: rerun this acceptance script and inspect the package releaser logs" >&2
  exit 4
fi
grep -Eq 'Package (review|release) requested changes' "$workspace_log"
test ! -e "$project_root/package.json"
if GIT_TERMINAL_PROMPT=0 git -C "$project_root" ls-remote origin >/dev/null 2>&1; then
  echo "Acceptance source origin unexpectedly became reachable" >&2
  echo "Repair: use an unreachable fixture origin and rerun this acceptance script" >&2
  exit 5
fi

tool_inventory=$(run_vm tools show release-tool)
grep -F '1.0.1 linux-arm64' <<< "$tool_inventory" >/dev/null
test "$(grep -c '  1.0.1 linux-' <<< "$tool_inventory")" = 2
run_vm --config "$consumer_root/vm.yaml" tools update "$consumer_environment"
test "$(docker exec --user acceptance "$consumer_environment" \
  /home/acceptance/.local/bin/release-tool --version)" = '1.0.1'

installed_state=$(docker exec --user acceptance "$consumer_environment" \
  cat /home/acceptance/.local/share/vm-tools/state/release-tool.state)
IFS=$'\t' read -r installed_tool installed_version installed_target installed_digest <<< "$installed_state"
test "$installed_tool" = release-tool
test "$installed_version" = 1.0.1
test "$installed_target" = linux-amd64
catalog_file=$(find "$acceptance_home" -type f -name 'index-linux-amd64.json' -print -quit)
test -n "$catalog_file"
catalog_digest=$(python3 -c \
  'import json,sys; print(json.load(open(sys.argv[1]))["tools"]["release-tool"]["artifact_digest"])' \
  "$catalog_file")
test "$installed_digest" = "$catalog_digest"

inventory_before=$(run_vm tools show release-tool)
docker exec --user acceptance "$environment_name" sh -ec \
  'cd /workspace && vm packages release && vm packages release'
run_vm --config "$consumer_root/vm.yaml" tools update "$consumer_environment"
run_vm --config "$consumer_root/vm.yaml" tools update "$consumer_environment"
test "$(run_vm tools show release-tool)" = "$inventory_before"
test "$(docker exec --user acceptance "$consumer_environment" \
  cat /home/acceptance/.local/share/vm-tools/state/release-tool.state)" = "$installed_state"

capture_runtime_state "$after_ids" "$after_volumes"
cmp "$before_ids" "$after_ids"
cmp "$before_volumes" "$after_volumes"

echo 'Docker package workflow acceptance passed'
