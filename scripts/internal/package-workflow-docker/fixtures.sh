fixture_assets=$script_dir/package-workflow-docker/fixtures

write_environment_config() {
  local root=$1
  local name=$2
  local selected_tool=${3:-}

  cat > "$root/vm.yaml" <<YAML
version: '2.0'
provider: docker
project:
  name: $name
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
tools:
  updates: auto
$selected_tool
YAML
}

initialize_fixture_repository() {
  local root=$1
  local message=$2

  git -C "$root" init --initial-branch main
  git -C "$root" config user.name 'VM Acceptance'
  git -C "$root" config user.email 'vm-acceptance@example.invalid'
  git -C "$root" add .
  git -C "$root" commit -m "$message"
}

prepare_source_discovery_fixtures() {
  local root
  for root in "$source_shelf/alias-a" "$source_shelf/alias-b"; do
    mkdir -p "$root"
    printf '%s\n' '{"name":"vm-acceptance-alias","version":"1.0.0"}' > "$root/package.json"
    initialize_fixture_repository "$root" 'test: add equivalent package source alias'
    git -C "$root" remote add origin ssh://git@example.invalid/shared-alias.git
  done

  root=$source_shelf/ignored
  mkdir -p "$root/duplicate"
  : > "$root/.vm-packages-ignore"
  printf '%s\n' '{"name":"vm-acceptance-ignored","version":"1.0.0"}' > "$root/duplicate/package.json"
  initialize_fixture_repository "$root/duplicate" 'test: add ignored package source'
  git -C "$root/duplicate" remote add origin ssh://git@example.invalid/ignored.git
}

prepare_registered_source_conflict_fixture() {
  local root=$source_shelf/archive/old-alias
  mkdir -p "$root"
  printf '%s\n' '{"name":"vm-acceptance-alias","version":"0.9.0"}' > "$root/package.json"
  initialize_fixture_repository "$root" 'test: add old package source identity'
  git -C "$root" remote add origin ssh://git@example.invalid/old-alias.git
}

prepare_acceptance_fixtures() {
  mkdir -p "$acceptance_home" "$project_root" "$consumer_root" \
    "$stopped_root" "$fixture_root" "$language_root" "$fake_bin"

  cp "$fixture_assets/fake-gh.sh" "$fake_bin/gh"
  chmod 0755 "$fake_bin/gh"

  for root in "$project_root" "$consumer_root" "$stopped_root"; do
    cp "$fixture_assets/environment.Dockerfile" "$root/Dockerfile.acceptance"
  done
  write_environment_config "$project_root" "$project_name" '  release-tool: {}'
  write_environment_config "$consumer_root" "$consumer_name" $'  release-tool: {}\n  agent-skills: {}'
  write_environment_config "$stopped_root" "$stopped_name" '  release-tool: {}'

  cp -R "$fixture_assets/release-tool/." "$project_root/"
  initialize_fixture_repository "$project_root" 'feat: initial binary tool'
  git -C "$project_root" remote add origin https://127.0.0.1:1/release-tool.git

  cp -R "$fixture_assets/collection/." "$fixture_root/"
  initialize_fixture_repository "$fixture_root" 'feat: initial collection'

  cp -R "$fixture_assets/language-package/." "$language_root/"
  initialize_fixture_repository "$language_root" 'feat: initial language package'
}

prepare_acceptance_infrastructure() {
  (
    cd "$project_root"
    gateway_port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')
    run_vm packages init "$source_shelf" \
      --engine docker \
      --port "$gateway_port" \
      --registry-image "$server_image" \
      --job-image "$jobs_image"
  )
  cat > "$acceptance_home/.vm/config.yaml" <<YAML
packages:
  source_roots:
    - $source_shelf
  canonical_sources:
    - $project_root
YAML
  prepare_source_discovery_fixtures
  run_vm packages up
  prepare_registered_source_conflict_fixture
  run_vm packages up
  test "$(run_vm packages status)" = 'Package infrastructure: healthy'
  test "$(run_vm packages list | grep -c 'vm-acceptance-alias')" = 1
  test "$(run_vm packages list | grep -c 'vm-acceptance-ignored' || true)" = 0

  run_project_vm "$project_root" shell --command true
  run_project_vm "$consumer_root" shell --command true
  run_project_vm "$stopped_root" shell --command true
  wait_for_guest_vm "$environment_name"
  wait_for_guest_vm "$consumer_environment"
  wait_for_guest_vm "$stopped_environment"
  run_project_vm "$stopped_root" stop

  docker run --rm --user 0:0 \
    --volume "${compose_project}_source-mirrors:/data/sources" \
    --volume "$fixture_root:/tool-fixture:ro" \
    --volume "$language_root:/language-fixture:ro" \
    --entrypoint /bin/sh "$server_image" -ec \
    'git clone --bare /tool-fixture /data/sources/acceptance-agent-skills.git &&
     git clone --bare /language-fixture /data/sources/acceptance-language.git &&
     chown -R 10001:10001 /data/sources/acceptance-agent-skills.git /data/sources/acceptance-language.git'
  run_vm tools register agent-skills --kind collection \
    --repository file:///data/sources/acceptance-agent-skills.git
  run_vm packages register vm-acceptance-language --ecosystem npm \
    --repository file:///data/sources/acceptance-language.git

  stable_containers=(
    "$compose_project-gateway-1"
    "$compose_project-oci-cache-1"
    "$compose_project-registry-1"
    "$compose_project-work-1"
    "$compose_project-build-edge-1"
    "$compose_project-reviewer-1"
    "$compose_project-builder-1"
    "$compose_project-releaser-1"
    "$compose_project-rollout-1"
    "$environment_name"
    "$edge_name"
    "$consumer_environment"
    "$consumer_edge"
    "$stopped_environment"
    "$stopped_edge"
  )
  workflow_containers=(
    "$compose_project-gateway-1"
    "$compose_project-work-1"
    "$compose_project-reviewer-1"
    "$compose_project-builder-1"
    "$compose_project-releaser-1"
    "$compose_project-rollout-1"
  )
  capture_runtime_state "$before_ids" "$before_volumes"
  project_manifest_digest=$(git -C "$project_root" hash-object package.json)
}
