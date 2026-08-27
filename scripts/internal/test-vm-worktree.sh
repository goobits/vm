#!/bin/bash

set -euo pipefail

TEST_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
TEST_TMP=$(mktemp -d)
trap 'rm -rf "$TEST_TMP"' EXIT

mkdir -p "$TEST_TMP/bin" "$TEST_TMP/worktrees" "$TEST_TMP/worktrees-sibling"
cat > "$TEST_TMP/bin/git" <<'EOF'
#!/bin/bash
case "${1:-} ${2:-}" in
    "rev-parse --git-dir")
        exit 0
        ;;
    "worktree list")
        exit 0
        ;;
    "worktree add")
        mkdir -p "$3"
        printf '%s\n' partial > "$3/partial-marker"
        exit 1
        ;;
    "worktree remove")
        exit 1
        ;;
esac
exit 1
EOF
chmod +x "$TEST_TMP/bin/git"
export PATH="$TEST_TMP/bin:$PATH"
export VM_WORKTREES="$TEST_TMP/worktrees"

mkdir -p "$VM_WORKTREES/existing"
printf '%s\n' keep > "$VM_WORKTREES/existing/owner-data"
if bash "$TEST_DIR/rust/vm-provider/src/container/vm-worktree.sh" add existing >/dev/null 2>&1; then
    echo "expected an existing non-worktree to be rejected" >&2
    exit 1
fi
test "$(cat "$VM_WORKTREES/existing/owner-data")" = keep

ln -s "$TEST_TMP/worktrees-sibling" "$VM_WORKTREES/escape"
if bash "$TEST_DIR/rust/vm-provider/src/container/vm-worktree.sh" add escape/child >/dev/null 2>&1; then
    echo "expected a symlink escape to be rejected" >&2
    exit 1
fi
test ! -e "$TEST_TMP/worktrees-sibling/child"

partial_output="$TEST_TMP/partial-output"
if bash "$TEST_DIR/rust/vm-provider/src/container/vm-worktree.sh" add partial >"$partial_output" 2>&1; then
    echo "expected fake worktree creation to fail" >&2
    exit 1
fi
if [ ! -f "$VM_WORKTREES/partial/partial-marker" ]; then
    echo "expected failed creation contents to be preserved" >&2
    cat "$partial_output" >&2
    exit 1
fi
grep -q "Leaving partial worktree in place" "$partial_output"

mkdir -p "$VM_WORKTREES/foo"
printf '%s\n' keep > "$VM_WORKTREES/foo/owner-data"
if bash "$TEST_DIR/rust/vm-provider/src/container/vm-worktree.sh" remove 'foo!' >/dev/null 2>&1; then
    echo "expected an invalid removal name to be rejected" >&2
    exit 1
fi
test "$(cat "$VM_WORKTREES/foo/owner-data")" = keep

echo "vm-worktree safety tests passed"
