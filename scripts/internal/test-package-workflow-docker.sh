#!/usr/bin/env bash
set -euo pipefail

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
  run_vm --config "$stopped_root/vm.yaml" remove "$stopped_environment" --force >/dev/null 2>&1
  docker rm --force "$environment_name" "$edge_name" \
    "$consumer_environment" "$consumer_edge" \
    "$stopped_environment" "$stopped_edge" >/dev/null 2>&1
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

source "$script_dir/package-workflow-docker/assertions.sh"
source "$script_dir/package-workflow-docker/fixtures.sh"
source "$script_dir/package-workflow-docker/language-package.sh"
source "$script_dir/package-workflow-docker/tool-release.sh"

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
docker build --provenance=false --tag "$server_image"   --file "$repository_root/rust/vm-package-server/docker/server/Dockerfile" "$repository_root"
docker build --provenance=false --tag "$jobs_image"   --file "$repository_root/rust/vm-package-jobs/Dockerfile" "$repository_root"

prepare_acceptance_fixtures
prepare_acceptance_infrastructure
accept_language_package_lifecycle
accept_tool_workflows

echo 'Docker package workflow acceptance passed'
