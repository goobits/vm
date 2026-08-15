#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
vm_binary=${VM_ACCEPTANCE_BIN:-$repository_root/rust/target/release/vm}
project_name=package-work-acceptance
environment_name=$project_name-dev
edge_name=$project_name-package-edge
acceptance_root=$(mktemp -d "${TMPDIR:-/tmp}/vm-package-acceptance.XXXXXX")
acceptance_home=$acceptance_root/home
project_root=$acceptance_root/project
source_shelf=$acceptance_root/sources
fixture_root=$acceptance_root/agent-skills
fake_bin=$acceptance_root/bin
work_log=$acceptance_root/work.log
before_ids=$acceptance_root/before.ids
after_ids=$acceptance_root/after.ids
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
  docker rm --force "$environment_name" "$edge_name" >/dev/null 2>&1
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

mkdir -p "$acceptance_home" "$project_root" "$source_shelf" "$fixture_root/skills/initial" "$fake_bin"

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
  name: package-work-acceptance
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
YAML

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

stable_containers="vm-packages-gateway-1 vm-packages-oci-cache-1 vm-packages-registry-1 vm-packages-work-1 vm-packages-reviewer-1 vm-packages-releaser-1 vm-packages-rollout-1 $environment_name $edge_name"
for container in $stable_containers; do
  docker inspect --format '{{.Name}} {{.Id}}' "$container"
done | sort > "$before_ids"

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

for container in $stable_containers; do
  docker inspect --format '{{.Name}} {{.Id}}' "$container"
done | sort > "$after_ids"
cmp "$before_ids" "$after_ids"

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

echo 'Docker package workflow acceptance passed'
