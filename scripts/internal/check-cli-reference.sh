#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
output="$repo_root/docs/user-guide/command-inventory.md"
temporary=$(mktemp)
trap 'rm -f "$temporary"' EXIT

cargo run --quiet --manifest-path "$repo_root/rust/Cargo.toml" \
  -p goobits-vm --example command_reference --all-features > "$temporary"
diff -u "$output" "$temporary"
