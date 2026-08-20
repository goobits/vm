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
prefix=vm-remote-acceptance-$run_id
image=$prefix:latest
network=$prefix
service=$prefix-service
project=$prefix-project
environment=$project-dev
unrelated=$prefix-unrelated-dev
guest_volume=$prefix-guest-home
unrelated_volume=$prefix-unrelated-home
service_volume=$prefix-service-state
acceptance_root=$(mktemp -d "${TMPDIR:-/tmp}/vm-remote-acceptance.XXXXXX")
acceptance_home=$acceptance_root/home
project_root=$acceptance_root/project
controller_registry=$acceptance_home/.vm/remote-commands.json

cleanup() {
  docker rm --force "$environment" "$unrelated" "$service" >/dev/null 2>&1 || true
  docker volume rm "$guest_volume" "$unrelated_volume" "$service_volume" >/dev/null 2>&1 || true
  docker network rm "$network" >/dev/null 2>&1 || true
  docker image rm --force "$image" >/dev/null 2>&1 || true
  case "$acceptance_root" in
    */vm-remote-acceptance.*) rm -rf -- "$acceptance_root" ;;
  esac
}
trap cleanup EXIT
trap 'exit 130' HUP INT TERM

run_vm() {
  env HOME="$acceptance_home" \
    VM_TEST_MODE=1 \
    VM_TEST_COMMAND_CONTEXT=host \
    VM_REMOTE_COMMANDS_CONTROLLER_FILE="$controller_registry" \
    "$vm_binary" --config "$project_root/vm.yaml" "$@"
}

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

mkdir -p "$acceptance_home/.vm" "$project_root"

cat > "$acceptance_root/Dockerfile" <<'DOCKERFILE'
FROM ubuntu:24.04
ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
      bash ca-certificates coreutils mount python3 sudo util-linux && \
    rm -rf /var/lib/apt/lists/* && \
    groupadd --gid 11000 acceptance && \
    useradd --create-home --uid 11000 --gid 11000 --shell /bin/bash acceptance && \
    printf '%s\n' 'acceptance ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/acceptance && \
    chmod 0440 /etc/sudoers.d/acceptance && \
    install -d -o acceptance -g acceptance /workspace
WORKDIR /workspace
CMD ["sleep", "infinity"]
DOCKERFILE

cat > "$acceptance_root/service.py" <<'PYTHON'
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

state = Path("/state/requests.jsonl")
opened = {}

class Handler(BaseHTTPRequestHandler):
    def reply(self, status, body):
        encoded = json.dumps(body, separators=(",", ":")).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def do_GET(self):
        self.reply(200 if self.path == "/health" else 404, {"ok": self.path == "/health"})

    def do_POST(self):
        if self.path != "/v1/commands/issue":
            self.reply(404, {})
            return
        if self.headers.get("Authorization") != "Bearer repository-scoped-capability":
            self.reply(401, {})
            return
        length = int(self.headers.get("Content-Length", "0"))
        request = json.loads(self.rfile.read(length))
        if set(request) != {"schema", "arguments", "idempotency_key"}:
            self.reply(400, {})
            return
        arguments = request["arguments"]
        record = {
            "arguments": arguments,
            "idempotency_key": request["idempotency_key"],
            "schema": request["schema"],
        }
        with state.open("a") as output:
            output.write(json.dumps(record, separators=(",", ":")) + "\n")
        if arguments == ["list"]:
            response = {"schema": 1, "exit_code": 0, "stdout": "#123 ready\n"}
        elif len(arguments) == 3 and arguments[0] == "open":
            key = tuple(arguments[1:])
            opened.setdefault(key, 124)
            response = {
                "schema": 1,
                "exit_code": 0,
                "stdout": f"#{opened[key]} {arguments[1]}\n",
            }
        else:
            response = {"schema": 1, "exit_code": 2, "stderr": "unsupported issue operation"}
        self.reply(200, response)

    def log_message(self, *_):
        pass

ThreadingHTTPServer(("0.0.0.0", 8080), Handler).serve_forever()
PYTHON

cat > "$project_root/vm.yaml" <<YAML
version: '2.0'
provider: docker
project:
  name: $project
  workspace_path: /workspace
vm:
  user: acceptance
  uid: 11000
  gid: 11000
YAML

cat > "$controller_registry" <<JSON
{
  "schema": 1,
  "environments": {
    "$environment": {
      "schema": 1,
      "commands": {
        "issue": {
          "endpoint": "http://$service:8080",
          "capability": "repository-scoped-capability",
          "repair_command": "vm start $environment"
        }
      }
    }
  }
}
JSON
chmod 0600 "$controller_registry"

docker build --provenance=false --tag "$image" --file "$acceptance_root/Dockerfile" "$acceptance_root"
docker network create "$network" >/dev/null
docker volume create "$guest_volume" >/dev/null
docker volume create "$unrelated_volume" >/dev/null
docker volume create "$service_volume" >/dev/null

docker run --detach --name "$service" --network "$network" \
  --volume "$service_volume:/state" \
  --volume "$acceptance_root/service.py:/service.py:ro" \
  "$image" python3 /service.py >/dev/null

for target in "$environment:$guest_volume:$project" "$unrelated:$unrelated_volume:unrelated"; do
  IFS=: read -r name volume label_project <<< "$target"
  docker run --detach --name "$name" --hostname "$name" --network "$network" \
    --label com.vm.managed=true \
    --label "com.vm.project=$label_project" \
    --label com.vm.role=environment \
    --volume "$volume:/home/acceptance" \
    --volume "$project_root:/workspace:ro" \
    --volume "$vm_binary:/usr/local/bin/vm:ro" \
    "$image" >/dev/null
done

ready=false
for _ in $(seq 1 50); do
  if docker exec "$service" python3 -c \
    'import urllib.request; urllib.request.urlopen("http://127.0.0.1:8080/health", timeout=1).read()' \
    >/dev/null 2>&1; then
    ready=true
    break
  fi
  sleep 0.2
done
test "$ready" = true || {
  echo "Remote command fixture did not become ready" >&2
  echo "Repair: rerun this acceptance test and inspect docker logs $service" >&2
  exit 1
}

before_ids=$(docker container inspect --format '{{.Id}}' "$service" "$environment" "$unrelated")
before_volumes=$(docker container inspect --format '{{range .Mounts}}{{if eq .Type "volume"}}{{.Name}}{{"\n"}}{{end}}{{end}}' \
  "$service" "$environment" "$unrelated" | sort)

run_vm tools update --to "$environment" >/dev/null
registry_digest=$(docker exec "$environment" sha256sum /etc/vm/remote-commands.json | awk '{print $1}')
run_vm tools update --to "$environment" >/dev/null
test "$(docker exec "$environment" sha256sum /etc/vm/remote-commands.json | awk '{print $1}')" = "$registry_digest"

run_vm tools update --to "$unrelated" >/dev/null
docker exec "$unrelated" test ! -e /etc/vm/remote-commands.json

docker exec "$environment" sh -ec '
  ! command -v git >/dev/null 2>&1
  ! env | grep -E "^(GH_TOKEN|GITHUB_TOKEN|GITLAB_TOKEN|NPM_TOKEN)=" >/dev/null
'

issue_list=$(docker exec --user acceptance --env HOME=/home/acceptance \
  "$environment" /usr/local/bin/vm issue list)
test "$issue_list" = '#123 ready'
first_open=$(docker exec --user acceptance --env HOME=/home/acceptance \
  "$environment" /usr/local/bin/vm issue open 'same title' 'same body')
second_open=$(docker exec --user acceptance --env HOME=/home/acceptance \
  "$environment" /usr/local/bin/vm issue open 'same title' 'same body')
test "$first_open" = '#124 same title'
test "$second_open" = "$first_open"

cp "$controller_registry" "$acceptance_root/registry.saved"
cat > "$controller_registry" <<'JSON'
{"schema":1,"environments":{}}
JSON
chmod 0600 "$controller_registry"
run_vm tools update --to "$environment" >/dev/null
docker exec "$environment" test ! -e /etc/vm/remote-commands.json
cp "$acceptance_root/registry.saved" "$controller_registry"
chmod 0600 "$controller_registry"
run_vm tools update --to "$environment" >/dev/null
test "$(docker exec --user acceptance --env HOME=/home/acceptance \
  "$environment" /usr/local/bin/vm issue list)" = '#123 ready'

after_ids=$(docker container inspect --format '{{.Id}}' "$service" "$environment" "$unrelated")
after_volumes=$(docker container inspect --format '{{range .Mounts}}{{if eq .Type "volume"}}{{.Name}}{{"\n"}}{{end}}{{end}}' \
  "$service" "$environment" "$unrelated" | sort)
test "$after_ids" = "$before_ids"
test "$after_volumes" = "$before_volumes"
request_count=$(docker exec "$service" sh -c 'wc -l < /state/requests.jsonl')
test "$request_count" -ge 4

echo 'Docker remote command acceptance passed'
