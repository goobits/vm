#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
vm_binary=${VM_ACCEPTANCE_BIN:-$repository_root/rust/target/release/vm}
run_id=$$
compose_project=vm-packages-acceptance-$run_id
docker_config=${DOCKER_CONFIG:-$HOME/.docker}
project_name=package-producer-acceptance-$run_id
environment_name=$project_name-dev
edge_name=$project_name-package-edge
consumer_name=package-consumer-acceptance-$run_id
consumer_environment=$consumer_name-dev
consumer_edge=$consumer_name-package-edge
acceptance_root=$(mktemp -d "${TMPDIR:-/tmp}/vm-package-acceptance.XXXXXX")
acceptance_home=$acceptance_root/home
source_shelf=$acceptance_root/sources
project_root=$acceptance_root/projects/release-tool
consumer_root=$acceptance_root/consumer
fixture_root=$acceptance_root/agent-skills
fake_bin=$acceptance_root/bin
checkout_log=$acceptance_root/checkout.log
workspace_log=$acceptance_root/workspace.log
before_ids=$acceptance_root/before.ids
after_ids=$acceptance_root/after.ids
before_volumes=$acceptance_root/before.volumes
after_volumes=$acceptance_root/after.volumes
server_image=
jobs_image=

run_vm() {
  env HOME="$acceptance_home" \
    DOCKER_CONFIG="$docker_config" \
    VM_PACKAGES_COMPOSE_PROJECT="$compose_project" \
    PATH="$fake_bin:$(dirname "$vm_binary"):$PATH" "$vm_binary" "$@"
}

cleanup() {
  set +e
  run_vm --config "$project_root/vm.yaml" remove "$environment_name" --force >/dev/null 2>&1
  run_vm --config "$consumer_root/vm.yaml" remove "$consumer_environment" --force >/dev/null 2>&1
  docker rm --force "$environment_name" "$edge_name" \
    "$consumer_environment" "$consumer_edge" >/dev/null 2>&1
  package_compose=$acceptance_home/.vm/infrastructure/packages/compose.yaml
  package_environment=$acceptance_home/.vm/infrastructure/packages/environment.env
  if test -f "$package_compose" && test -f "$package_environment"; then
    docker compose --project-name "$compose_project" --file "$package_compose" \
      --env-file "$package_environment" down --volumes --remove-orphans >/dev/null 2>&1
  fi
  docker network rm "$compose_project" "$compose_project-controller" \
    "$compose_project-egress" >/dev/null 2>&1
  if test -n "$server_image" && test -n "$jobs_image"; then
    docker image rm --force "$server_image" "$jobs_image" >/dev/null 2>&1
  fi
  case "$acceptance_root" in
    */vm-package-acceptance.*) rm -rf -- "$acceptance_root" ;;
  esac
}
trap cleanup EXIT
trap 'exit 130' HUP INT TERM

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
server_image=vm-package-server-acceptance:$version-$run_id
jobs_image=vm-package-jobs-acceptance:$version-$run_id
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
ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
      ansible-core ca-certificates curl git nodejs npm python3 sudo bash tar gzip && \
    rm -rf /var/lib/apt/lists/* && \
    groupadd --gid 11000 acceptance && \
    useradd --create-home --uid 11000 --gid 11000 --shell /bin/bash acceptance && \
    printf '%s\n' 'acceptance ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/acceptance && \
    chmod 0440 /etc/sudoers.d/acceptance && \
    install -d -o acceptance -g acceptance /workspace && \
    sudo -Hu acceptance git config --global user.name 'VM Acceptance' && \
    sudo -Hu acceptance git config --global user.email 'vm-acceptance@example.invalid'
USER acceptance
WORKDIR /workspace
CMD ["tail", "-f", "/dev/null"]
DOCKERFILE

cat > "$project_root/vm.yaml" <<YAML
version: '2.0'
provider: docker
project:
  name: $project_name
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

cp "$project_root/Dockerfile.acceptance" "$consumer_root/Dockerfile.acceptance"
cat > "$consumer_root/vm.yaml" <<YAML
version: '2.0'
provider: docker
project:
  name: $consumer_name
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
for (const key of Object.keys(process.env)) {
  if (key.includes('TOKEN') || key.includes('SECRET')) process.exit(8);
}
for (const secret of [
  '/run/secrets/build_token',
  '/run/build-secrets/build-token',
  '/run/secrets/release_token',
  '/run/secrets/publish_token',
  '/run/secrets/git_token',
]) {
  try {
    readFileSync(secret);
    process.exit(9);
  } catch {}
}
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
  gateway_port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')
  run_vm packages init "$source_shelf" \
    --runtime docker \
    --port "$gateway_port" \
    --registry-image "$server_image" \
    --job-image "$jobs_image"
)
run_vm packages register "$project_root"
test "$(run_vm packages status)" = 'Package infrastructure: healthy'

run_vm --config "$project_root/vm.yaml" create
run_vm --config "$consumer_root/vm.yaml" create

docker run --rm --user 0:0 \
  --volume "${compose_project}_source-mirrors:/data/sources" \
  --volume "$fixture_root:/fixture:ro" \
  --entrypoint /bin/sh "$server_image" -ec \
  'git clone --bare /fixture /data/sources/acceptance-agent-skills.git && chown -R 10001:10001 /data/sources/acceptance-agent-skills.git'
run_vm tools register agent-skills --kind collection \
  --repository file:///data/sources/acceptance-agent-skills.git

stable_containers=(
  "$compose_project-gateway-1"
  "$compose_project-oci-cache-1"
  "$compose_project-registry-1"
  "$compose_project-work-1"
  "$compose_project-reviewer-1"
  "$compose_project-builder-1"
  "$compose_project-releaser-1"
  "$compose_project-rollout-1"
  "$environment_name"
  "$edge_name"
  "$consumer_environment"
  "$consumer_edge"
)
capture_runtime_state "$before_ids" "$before_volumes"

workflow_containers=(
  "$compose_project-gateway-1"
  "$compose_project-work-1"
  "$compose_project-reviewer-1"
  "$compose_project-builder-1"
  "$compose_project-releaser-1"
  "$compose_project-rollout-1"
)

docker exec --user acceptance "$environment_name" \
  vm packages checkout agent-skills >"$checkout_log" 2>&1
checkout_source=$(sed -n 's/^Source: //p' "$checkout_log")
test "$(printf '%s\n' "$checkout_source" | sed '/^$/d' | wc -l | tr -d ' ')" = 1
case "$checkout_source" in
  /home/acceptance/.local/share/vm/package-checkouts/*/source) ;;
  *)
    cat "$checkout_log" >&2
    echo "Managed checkout escaped guest storage: $checkout_source" >&2
    echo "Repair: rerun this acceptance script with a guest-owned checkout build" >&2
    exit 3
    ;;
esac
checkout_id=$(basename "$(dirname "$checkout_source")")
docker exec --user acceptance "$environment_name" test -d "$checkout_source/.git"

assert_guest_only_checkout() {
  local container
  for container in "${workflow_containers[@]}"; do
    docker exec "$container" sh -ec \
      'test ! -e "$1" && test ! -e "$2"' sh \
      "$checkout_source" "/data/agents/$checkout_id/source"
  done
}
assert_guest_only_checkout

docker exec --user acceptance "$environment_name" sh -ec '
  source=$1
  mkdir -p "$source/skills/acceptance"
  printf "%s\n" "# Guest-owned acceptance skill" > "$source/skills/acceptance/SKILL.md"
  cd "$source"
  test -f skills/acceptance/SKILL.md
  git add skills/acceptance/SKILL.md
  git commit -m "feat: add guest-owned acceptance skill"
' sh "$checkout_source"
checkout_commit=$(docker exec --user acceptance "$environment_name" \
  git -C "$checkout_source" rev-parse HEAD)

docker restart "$environment_name" >/dev/null
docker exec --user acceptance "$environment_name" \
  vm packages checkout agent-skills >"$checkout_log.resume" 2>&1
resumed_source=$(sed -n 's/^Source: //p' "$checkout_log.resume")
test "$resumed_source" = "$checkout_source"
test "$(docker exec --user acceptance "$environment_name" \
  git -C "$checkout_source" rev-parse HEAD)" = "$checkout_commit"
test -z "$(docker exec --user acceptance "$environment_name" \
  git -C "$checkout_source" status --porcelain --untracked-files=all)"
assert_guest_only_checkout

set +e
docker exec --user acceptance "$environment_name" sh -ec \
  'cd "$1" && vm packages release' sh "$checkout_source" \
  >"$workspace_log" 2>&1
release_status=$?
set -e
test "$release_status" -ne 0 || {
  cat "$workspace_log" >&2
  echo "Guest release unexpectedly skipped the version rework" >&2
  echo "Repair: rerun this acceptance script with package review enabled" >&2
  exit 4
}
grep -Eq 'Package (review|release) requested changes' "$workspace_log"
assert_guest_only_checkout

docker exec --user acceptance "$environment_name" sh -ec '
  source=$1
  sed -i '\''s/"version": "1.0.0"/"version": "1.0.1"/'\'' "$source/package.json"
  cd "$source"
  test "$(sed -n '\''s/.*"version": "\([^"]*\)".*/\1/p'\'' package.json)" = 1.0.1
  git add package.json
  git commit -m "fix: apply requested collection version bump"
  vm packages release
' sh "$checkout_source" >>"$workspace_log" 2>&1
docker exec --user acceptance "$environment_name" test ! -e "$checkout_source"
assert_guest_only_checkout

for container in "${workflow_containers[@]}"; do
  if docker inspect --format '{{range .Mounts}}{{println .Source}}{{end}}' "$container" | \
    grep -Fx "$project_root" >/dev/null; then
    echo "Package infrastructure mounted the canonical project workspace in $container" >&2
    exit 5
  fi
  if docker top "$container" -eo args | grep -E '[c]odex|[c]laude|[a]ntigravity' >/dev/null; then
    echo "Package appliance launched an AI agent in $container" >&2
    echo "Repair: remove agent launchers from package infrastructure" >&2
    exit 5
  fi
done
docker exec --user acceptance "$environment_name" sh -ec '
  if env | grep -Eq "^(PKG_WORK_CONTROLLER|PKG_WORK_GIT|PKG_BUILD|PKG_RELEASE|PKG_SERVER_PUBLISH|GIT_ASKPASS|GITHUB_TOKEN|GH_TOKEN|NPM_TOKEN)[^=]*="; then
    exit 1
  fi
  for secret in \
    /run/secrets/git_token \
    /run/secrets/controller_token \
    /run/secrets/build_token \
    /run/build-secrets/build-token \
    /run/secrets/release_token \
    /run/secrets/publish_token; do
    test ! -e "$secret"
  done
'
if docker inspect --format '{{range .Mounts}}{{println .Name}}{{end}}' "$environment_name" | \
  grep -F "${compose_project}_" >/dev/null; then
  echo "Producer guest received writable package-appliance storage" >&2
  echo "Repair: remove appliance volume mounts from managed guests" >&2
  exit 6
fi

docker exec --user acceptance "$environment_name" sh -ec '
  clone=/home/acceptance/unregistered-release-clone
  rm -rf "$clone"
  git clone /workspace "$clone" >/dev/null 2>&1
  cd "$clone"
  if vm packages release >/tmp/unregistered-release.log 2>&1; then
    echo "Unregistered same-origin clone unexpectedly received release authority" >&2
    exit 1
  fi
  grep -F "not the configured canonical workspace" /tmp/unregistered-release.log >/dev/null
  rm -rf "$clone" /tmp/unregistered-release.log
'

run_vm --config "$consumer_root/vm.yaml" tools update --to "$consumer_environment"
docker exec --user acceptance "$consumer_environment" \
  test -L /home/acceptance/.codex/skills/acceptance
docker exec --user acceptance "$consumer_environment" \
  grep -F 'Guest-owned acceptance skill' \
  /home/acceptance/.codex/skills/acceptance/SKILL.md >/dev/null
run_vm tools show agent-skills | grep -F '1.0.1' >/dev/null
docker run --rm --user 10001:10001 \
  --volume "${compose_project}_source-mirrors:/data/sources:ro" \
  --entrypoint git "$server_image" \
  --git-dir=/data/sources/acceptance-agent-skills.git show main:package.json | \
  grep -F '"version": "1.0.1"' >/dev/null

if ! docker exec --user acceptance "$environment_name" sh -ec '
  cd /workspace
  test -z "$(git status --porcelain --untracked-files=all)"
  initial=$(git rev-parse HEAD)
  vm packages release
  test "$(git rev-parse HEAD)" = "$initial"
  test -z "$(git status --porcelain --untracked-files=all)"
  test -z "$(git tag --list)"
  printf "%s\n" "workspace release change" >> README.md
  git add README.md
  git commit -m "feat: update binary behavior"
  unchanged=$(git rev-parse HEAD)
  if vm packages release; then
    echo "Later workspace release unexpectedly succeeded without a version bump" >&2
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
run_vm --config "$consumer_root/vm.yaml" tools update --to "$consumer_environment"
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
run_vm --config "$consumer_root/vm.yaml" tools update --to "$consumer_environment"
run_vm --config "$consumer_root/vm.yaml" tools update --to "$consumer_environment"
test "$(run_vm tools show release-tool)" = "$inventory_before"
test "$(docker exec --user acceptance "$consumer_environment" \
  cat /home/acceptance/.local/share/vm-tools/state/release-tool.state)" = "$installed_state"

capture_runtime_state "$after_ids" "$after_volumes"
cmp "$before_ids" "$after_ids"
cmp "$before_volumes" "$after_volumes"

echo 'Docker package workflow acceptance passed'
