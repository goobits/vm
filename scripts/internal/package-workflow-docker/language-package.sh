release_source_only_language_package() {
  local checkout_id source

  # Release from a source-only checkout: without a registered dependency, no
  # project override should be created or restored.
  docker exec --user acceptance "$environment_name" \
    vm packages checkout vm-acceptance-language >"$language_log" 2>&1
  source=$(checkout_source_from_log "$language_log")
  checkout_id=$(basename "$(dirname "$source")")
  run_vm packages show "$checkout_id" | grep -F '"source_only": true' >/dev/null
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
  run_vm packages consumer register "$project_name" \
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
  run_vm packages show "$checkout_id" | grep -F '"source_only": false' >/dev/null
  docker exec --user acceptance "$environment_name" \
    test -f "$(dirname "$source")/override.json"
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
  run_vm packages show "$checkout_id" | grep -F '"state": "cancelled"' >/dev/null
  docker exec --user acceptance "$environment_name" sh -ec '
    test "$(cat /workspace/node_modules/vm-acceptance-language/source-marker.txt)" = checkout-only
    sed -i '\''s/"pinned_version": "9.9.9"/"pinned_version": "1.0.1"/'\'' "$(dirname "$1")/override.json"
    cd "$1"
    vm packages cancel
  ' sh "$source" >>"$language_log.cancel" 2>&1
  docker exec --user acceptance "$environment_name" test ! -e "$source"
  run_vm packages show "$checkout_id" | grep -F '"state": "closed"' >/dev/null
  docker exec --user acceptance "$environment_name" sh -ec '
    cd /workspace
    test "$(node -p "require('\''./node_modules/vm-acceptance-language/package.json'\'').version")" = 1.0.1
    test "$(cat node_modules/vm-acceptance-language/source-marker.txt)" = published
    test ! -e package-lock.json
  '
}

accept_language_package_lifecycle() {
  release_source_only_language_package
  assert_language_dependency_restoration
  test "$(git -C "$project_root" hash-object package.json)" = "$project_manifest_digest"
  test -z "$(git -C "$project_root" status --porcelain --untracked-files=all)"
}
