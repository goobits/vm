release_source_only_language_package() {
  local checkout_id source

  # Release from a source-only checkout: without a registered dependency, no
  # project override should be created or restored.
  docker exec --user acceptance "$environment_name" \
    vm packages checkout vm-acceptance-language >"$language_log" 2>&1
  source=$(checkout_source_from_log "$language_log")
  checkout_id=$(basename "$(dirname "$source")")
  assert_checkout_field "$checkout_id" source_only true
  docker exec --user acceptance "$environment_name" \
    test ! -e "$(dirname "$source")/override.json"
  docker exec --user acceptance "$environment_name" sh -ec '
    source=$1
    cd "$source"
    sed -i '\''s/"version": "1.0.0"/"version": "1.0.1"/'\'' package.json
    git add package.json
    git commit -m "feat: publish source-only language package"
    vm packages release
  ' sh "$source" >>"$language_log" 2>&1
  docker exec --user acceptance "$environment_name" test ! -e "$source"
  docker exec --user acceptance "$environment_name" sh -ec '
    test "$(npm view vm-acceptance-language@1.0.1 version)" = 1.0.1
  '
}

assert_language_dependency_restoration() {
  local cancel_status checkout_id source

  # Add a durable dependency pin, activate a checkout override, then prove a
  # failed restoration retains the checkout in cancelled state. Retrying after
  # repair must restore the published dependency before durable closure.
  run_vm packages consumers register "$project_name" \
    --repository "https://example.invalid/$project_name.git" \
    --dependency vm-acceptance-language@1.0.1
  docker exec --user acceptance "$environment_name" sh -ec '
    cd /workspace
    npm install --no-save --package-lock=false vm-acceptance-language@1.0.1
    test "$(node -p "require('\''./node_modules/vm-acceptance-language/package.json'\'').version")" = 1.0.1
    test "$(cat node_modules/vm-acceptance-language/source-marker.txt)" = published
  '
  docker exec --user acceptance "$environment_name" \
    vm packages checkout vm-acceptance-language >"$language_log.cancel" 2>&1
  source=$(checkout_source_from_log "$language_log.cancel")
  checkout_id=$(basename "$(dirname "$source")")
  assert_checkout_field "$checkout_id" source_only false || {
    echo "Managed dependency checkout was incorrectly marked source-only" >&2
    return 1
  }
  docker exec --user acceptance "$environment_name" sh -ec '
    test -f "$1" || {
      echo "Managed dependency checkout has no override record: $1" >&2
      ls -la "$(dirname "$1")" >&2
      exit 1
    }
  ' sh "$(dirname "$source")/override.json"
  docker exec --user acceptance "$environment_name" sh -ec '
    printf "%s\n" checkout-only > "$1/source-marker.txt"
    test "$(cat /workspace/node_modules/vm-acceptance-language/source-marker.txt)" = checkout-only
    sed -i '\''s/"pinned_version": "1.0.1"/"pinned_version": "9.9.9"/'\'' "$(dirname "$1")/override.json"
  ' sh "$source"

  set +e
  docker exec --user acceptance "$environment_name" sh -ec \
    'cd "$1" && vm packages cancel' sh "$source" \
    >>"$language_log.cancel" 2>&1
  cancel_status=$?
  set -e
  test "$cancel_status" -ne 0 || {
    cat "$language_log.cancel" >&2
    echo "Checkout closed even though dependency restoration failed" >&2
    exit 4
  }
  docker exec --user acceptance "$environment_name" test -d "$source"
  assert_checkout_field "$checkout_id" state cancelled
  docker exec --user acceptance "$environment_name" sh -ec '
    test "$(cat /workspace/node_modules/vm-acceptance-language/source-marker.txt)" = checkout-only
    sed -i '\''s/"pinned_version": "9.9.9"/"pinned_version": "1.0.1"/'\'' "$(dirname "$1")/override.json"
    cd "$1"
    vm packages cancel
  ' sh "$source" >>"$language_log.cancel" 2>&1
  docker exec --user acceptance "$environment_name" test ! -e "$source"
  assert_checkout_field "$checkout_id" state closed
  docker exec --user acceptance "$environment_name" sh -ec '
    cd /workspace
    restored=node_modules/vm-acceptance-language
    if ! test "$(node -p "require('\''./node_modules/vm-acceptance-language/package.json'\'').version" 2>/dev/null)" = 1.0.1 ||
      ! test "$(cat "$restored/source-marker.txt" 2>/dev/null)" = published
    then
      echo "Published dependency was not restored after checkout cancellation" >&2
      ls -ld node_modules "$restored" 2>&1 || true
      readlink "$restored" 2>&1 || true
      find node_modules -maxdepth 2 -type f -o -type l 2>/dev/null | sort >&2 || true
      exit 1
    fi
  '
}

accept_language_package_lifecycle() {
  prepare_language_consumer
  release_source_only_language_package
  assert_language_consumer_review
  assert_language_dependency_restoration
  assert_project_dependency_files_unchanged
}

prepare_language_consumer() {
  local root=$acceptance_root/review-consumer
  mkdir -p "$root"
  cat > "$root/package.json" <<'JSON'
{"name":"acceptance-review-consumer","version":"1.0.0","private":true,"dependencies":{"vm-acceptance-language":"1.0.0"},"scripts":{"test":"node -e \"if(require('vm-acceptance-language/package.json').version !== '1.0.1') process.exit(1)\""}}
JSON
  initialize_fixture_repository "$root" 'test: add consumer awaiting reviewed package update'
  docker run --rm --user 0:0 \
    --volume "${compose_project}_source-mirrors:/data/sources" \
    --volume "$root:/consumer-fixture:ro" \
    --entrypoint /bin/sh "$server_image" -ec '
      git config --global --add safe.directory /consumer-fixture/.git
      git clone --bare /consumer-fixture /data/sources/acceptance-review-consumer.git
      chown -R 10001:10001 /data/sources/acceptance-review-consumer.git
    '
  run_vm packages consumers register acceptance-review-consumer \
    --repository file:///data/sources/acceptance-review-consumer.git \
    --dependency vm-acceptance-language@1.0.0
}

assert_language_consumer_review() {
  local attempt branch ready=false
  for attempt in $(seq 1 300); do
    if workflow_state | python3 -c '
import json,sys
state=json.load(sys.stdin)
rollouts=[r for r in state["rollouts"].values()
          if r["consumer"] == "acceptance-review-consumer"]
if any(r["state"] == "failed" for r in rollouts):
    print(rollouts, file=sys.stderr)
    sys.exit(2)
assert len(rollouts) == 1 and rollouts[0]["state"] == "ready_for_review", rollouts
rollout=rollouts[0]
assert rollout["submitted_commit"] and rollout["branch"], rollout
assert all(t["receipt_id"] for t in rollout["transitions"]), rollout
assert state["consumers"]["acceptance-review-consumer"]["dependencies"]["vm-acceptance-language"] == "1.0.0"
print(rollout["branch"])
' >"$language_log.review" 2>"$language_log.review.error"; then
      ready=true
      break
    elif test "$?" -eq 2; then
      cat "$language_log.review.error" >&2
      return 1
    fi
    sleep 1
  done
  test "$ready" = true || { cat "$language_log.review.error" >&2; return 1; }
  branch=$(cat "$language_log.review")
  docker run --rm --user 10001:10001 \
    --volume "${compose_project}_source-mirrors:/data/sources:ro" \
    --entrypoint /bin/sh "$server_image" -ec '
      repository=/data/sources/acceptance-review-consumer.git
      git --git-dir="$repository" show main:package.json
      git --git-dir="$repository" show "$1:package.json"
    ' sh "$branch" >"$language_log.review.manifests"
  python3 -c '
import json,sys
with open(sys.argv[1]) as source:
    decoder=json.JSONDecoder()
    content=source.read().lstrip()
    original,end=decoder.raw_decode(content)
    updated,_=decoder.raw_decode(content[end:].lstrip())
assert original["dependencies"]["vm-acceptance-language"] == "1.0.0"
assert updated["dependencies"]["vm-acceptance-language"].lstrip("^~") == "1.0.1"
' "$language_log.review.manifests"
  run_vm packages consumers retry acceptance-review-consumer >>"$language_log.review" 2>&1
  grep -F 'No failed dependency updates need retry for acceptance-review-consumer' \
    "$language_log.review" >/dev/null
}
