#!/usr/bin/env bash
set -Eeuo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
if test -n "${VM_ACCEPTANCE_BIN:-}"; then
  vm_binary=$VM_ACCEPTANCE_BIN
else
  cargo_target_dir=${CARGO_TARGET_DIR:-$(
    cd "$repository_root/rust"
    cargo metadata --no-deps --format-version 1 | \
      sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p'
  )}
  vm_binary=$cargo_target_dir/release/vm
fi
run_id=$$
compose_project=vm-packages-acceptance-$run_id
docker_config=${DOCKER_CONFIG:-$HOME/.docker}
project_name=package-producer-acceptance-$run_id
environment_name=$project_name-dev
edge_name=$project_name-package-edge
consumer_name=package-consumer-acceptance-$run_id
consumer_environment=$consumer_name-dev
consumer_edge=$consumer_name-package-edge
stopped_name=package-stopped-acceptance-$run_id
stopped_environment=$stopped_name-dev
stopped_edge=$stopped_name-package-edge
acceptance_root=$(mktemp -d "${TMPDIR:-/tmp}/vm-package-acceptance.XXXXXX")
acceptance_home=$acceptance_root/home
source_shelf=$acceptance_root/sources
project_root=$acceptance_root/projects/release-tool
consumer_root=$acceptance_root/consumer
stopped_root=$acceptance_root/stopped
fixture_root=$acceptance_root/agent-skills
language_root=$acceptance_root/language-package
fake_bin=$acceptance_root/bin
checkout_log=$acceptance_root/checkout.log
language_log=$acceptance_root/language.log
workspace_log=$acceptance_root/workspace.log
activation_log=$acceptance_root/activation.log
docker_restart_command=${VM_ACCEPTANCE_DOCKER_RESTART_COMMAND:-}
before_ids=$acceptance_root/before.ids
after_ids=$acceptance_root/after.ids
before_volumes=$acceptance_root/before.volumes
after_volumes=$acceptance_root/after.volumes
server_image=
jobs_image=
acceptance_phase=initialize
failure_line=unknown

run_vm() {
  env HOME="$acceptance_home" \
    DOCKER_CONFIG="$docker_config" \
    VM_PACKAGES_COMPOSE_PROJECT="$compose_project" \
    PATH="$fake_bin:$(dirname "$vm_binary"):$PATH" "$vm_binary" "$@"
}

run_project_vm() {
  local project=$1
  shift
  (cd "$project" && run_vm "$@")
}

capture_failure_evidence() {
  local status=$1
  local evidence=$acceptance_root/failure-evidence
  local container
  mkdir -p "$evidence/containers"
  printf 'exit_status=%s\nphase=%s\nline=%s\n' \
    "$status" "$acceptance_phase" "$failure_line" > "$evidence/status"
  docker ps --all --no-trunc > "$evidence/docker-ps.txt" 2>&1 || true
  docker info > "$evidence/docker-info.txt" 2>&1 || true
  workflow_state > "$evidence/workflows.json" 2> "$evidence/workflows.error" || true
  while IFS= read -r container; do
    test -n "$container" || continue
    docker container inspect "$container" \
      > "$evidence/containers/$container.inspect.json" 2>&1 || true
    docker logs "$container" \
      > "$evidence/containers/$container.log" 2>&1 || true
  done < <(docker ps --all --format '{{.Names}}' 2>/dev/null | \
    grep -E "^(${compose_project}|${project_name}|${consumer_name}|${stopped_name})" || true)
}

cleanup_environment_resources() {
  local project container volume network
  for project in "$project_name" "$consumer_name" "$stopped_name"; do
    case "$project" in
      package-*-acceptance-[0-9]*) ;;
      *)
        echo "Refusing to clean unexpected acceptance project: $project" >&2
        continue
        ;;
    esac
    while IFS= read -r container; do
      test -z "$container" || docker rm --force "$container" >/dev/null 2>&1
    done < <(docker ps --all --quiet \
      --filter "label=com.docker.compose.project=$project")
    while IFS= read -r volume; do
      test -z "$volume" || docker volume rm "$volume" >/dev/null 2>&1
    done < <(docker volume ls --quiet --filter "label=com.vm.project=$project")
    while IFS= read -r network; do
      test -z "$network" || docker network rm "$network" >/dev/null 2>&1
    done < <(docker network ls --quiet \
      --filter "label=com.docker.compose.project=$project")
  done
}

cleanup() {
  local status=$?
  trap - EXIT
  set +e
  if test "$status" -ne 0; then
    capture_failure_evidence "$status"
  fi
  cleanup_environment_resources
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
  if test "$status" -eq 0; then
    case "$acceptance_root" in
      */vm-package-acceptance.*) find "$acceptance_root" -depth -delete ;;
    esac
  else
    echo "Docker package workflow evidence preserved: $acceptance_root" >&2
  fi
  exit "$status"
}
trap cleanup EXIT
trap 'failure_line=$LINENO' ERR
trap 'exit 130' HUP INT TERM

source "$script_dir/package-workflow-docker/assertions.sh"
source "$script_dir/package-workflow-docker/fixtures.sh"
source "$script_dir/package-workflow-docker/language-package.sh"
source "$script_dir/package-workflow-docker/tool-release.sh"

acceptance_phase=check-prerequisites
test -x "$vm_binary" || {
  echo "Acceptance VM binary is missing: $vm_binary" >&2
  echo "Repair: (cd rust && cargo build --release --package goobits-vm)" >&2
  exit 2
}
command -v docker >/dev/null 2>&1 || {
  echo "Docker is unavailable" >&2
  echo "Repair: install Docker Engine and rerun this script" >&2
  exit 2
}

version=$("$vm_binary" --version | awk '{print $2}')
server_image=vm-package-server-acceptance:$version-$run_id
jobs_image=vm-package-jobs-acceptance:$version-$run_id
acceptance_phase=build-appliance-images
docker build --provenance=false --tag "$server_image"   --file "$repository_root/rust/vm-package-server/docker/server/Dockerfile" "$repository_root"
docker build --provenance=false --tag "$jobs_image"   --file "$repository_root/rust/vm-package-jobs/Dockerfile" "$repository_root"

acceptance_phase=prepare-fixtures
prepare_acceptance_fixtures
acceptance_phase=prepare-infrastructure
prepare_acceptance_infrastructure
acceptance_phase=language-package-lifecycle
accept_language_package_lifecycle
acceptance_phase=managed-tool-workflows
accept_tool_workflows

acceptance_phase=complete
echo 'Docker package workflow acceptance passed'
