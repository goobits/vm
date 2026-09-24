#!/bin/bash

set -euo pipefail

TEST_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
TEST_TMP=$(mktemp -d)
trap 'rm -rf "$TEST_TMP"' EXIT

export HOME="$TEST_TMP/home"
mkdir -p "$HOME" "$TEST_TMP/bin"

cat > "$TEST_TMP/bin/curl" <<'EOF'
#!/bin/bash
previous=""
for argument in "$@"; do
    if [[ "$previous" == "--output" ]]; then
        cp "$FAKE_RUSTUP_BINARY" "$argument"
        exit 0
    fi
    previous="$argument"
done
if [[ "${FAKE_CHECKSUM_UNAVAILABLE:-}" == "yes" ]]; then
    exit 22
fi
printf '%s\n' "${FAKE_CHECKSUM_RESPONSE:-}"
EOF
cat > "$TEST_TMP/bin/logger" <<'EOF'
#!/bin/bash
exit 0
EOF
chmod +x "$TEST_TMP/bin/curl" "$TEST_TMP/bin/logger"
export PATH="$TEST_TMP/bin:$PATH"

# shellcheck source=../../install.sh
source "$TEST_DIR/install.sh"
initialize_log_file
ARCH=x86_64
OS_TYPE=linux

fixture="$TEST_TMP/rustup-init"
printf '%s\n' 'trusted rustup fixture' > "$fixture"
if command -v sha256sum >/dev/null 2>&1; then
    fixture_hash=$(sha256sum "$fixture" | awk '{print $1}')
else
    fixture_hash=$(shasum -a 256 "$fixture" | awk '{print $1}')
fi

FAKE_CHECKSUM_RESPONSE="$fixture_hash  rustup-init"
export FAKE_CHECKSUM_RESPONSE
verify_rustup_checksum "$fixture"

expected_url="https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init"
if [[ "$(rustup_init_url)" != "$expected_url" ]]; then
    echo "rustup download URL does not identify the verified rustup-init artifact" >&2
    exit 1
fi

ARCH=aarch64
OS_TYPE=macos
if [[ "$(rustup_target)" != "aarch64-apple-darwin" ]]; then
    echo "expected Apple Silicon rustup target" >&2
    exit 1
fi

ARCH=x86_64
if [[ "$(rustup_target)" != "x86_64-apple-darwin" ]]; then
    echo "expected Intel Mac rustup target" >&2
    exit 1
fi

OS_TYPE=alpine
ARCH=aarch64
if [[ "$(rustup_target)" != "aarch64-unknown-linux-musl" ]]; then
    echo "expected Alpine ARM64 rustup target" >&2
    exit 1
fi

ARCH=x86_64
OS_TYPE=linux

FAKE_CHECKSUM_RESPONSE=not-a-sha256
export FAKE_CHECKSUM_RESPONSE
if verify_rustup_checksum "$fixture"; then
    echo "expected malformed checksum to fail" >&2
    exit 1
fi

FAKE_CHECKSUM_RESPONSE=
FAKE_CHECKSUM_UNAVAILABLE=yes
export FAKE_CHECKSUM_RESPONSE FAKE_CHECKSUM_UNAVAILABLE
if verify_rustup_checksum "$fixture"; then
    echo "expected unavailable checksum to fail" >&2
    exit 1
fi

rm -f "$LOG_FILE"
log_target="$TEST_TMP/log-target"
printf '%s\n' preserved > "$log_target"
ln -s "$log_target" "$LOG_FILE"
if initialize_log_file; then
    echo "expected symlinked installer log to be rejected" >&2
    exit 1
fi
if [[ "$(cat "$log_target")" != preserved ]]; then
    echo "installer log initialization changed a symlink target" >&2
    exit 1
fi
rm -f "$LOG_FILE"
initialize_log_file

# macOS has no timeout command by default; commands must still run there.
command_exists() {
    case "$1" in
        cargo|timeout|gtimeout) return 1 ;;
        *) command -v "$1" >/dev/null 2>&1 ;;
    esac
}
if [[ "$(run_with_timeout 1 printf portable)" != portable ]]; then
    echo "expected timeout-free command fallback to run" >&2
    exit 1
fi

# rustup uses its executable filename to select installer mode.
cat > "$TEST_TMP/fake-rustup-init" <<'EOF'
#!/bin/bash
[[ "${0##*/}" == "rustup-init" ]] || exit 1
[[ "$1" == "-y" && "$2" == "--no-modify-path" ]] || exit 1
mkdir -p "$CARGO_HOME"
printf '%s\n' '# fake cargo environment' > "$CARGO_HOME/env"
EOF
chmod +x "$TEST_TMP/fake-rustup-init"
FAKE_RUSTUP_BINARY="$TEST_TMP/fake-rustup-init"
CARGO_HOME="$TEST_TMP/custom-cargo"
FAKE_CHECKSUM_RESPONSE="$(sha256sum "$FAKE_RUSTUP_BINARY" 2>/dev/null || shasum -a 256 "$FAKE_RUSTUP_BINARY")"
FAKE_CHECKSUM_RESPONSE="${FAKE_CHECKSUM_RESPONSE%% *}  rustup-init"
FAKE_CHECKSUM_UNAVAILABLE=no
export FAKE_RUSTUP_BINARY FAKE_CHECKSUM_RESPONSE FAKE_CHECKSUM_UNAVAILABLE CARGO_HOME
( install_rust_secure )

# An existing Rust toolchain outside PATH must be reused without a download.
mkdir -p "$CARGO_HOME/bin"
printf '%s\n' '#!/bin/sh' 'echo cargo 1.98.1' > "$CARGO_HOME/bin/cargo"
printf '%s\n' '#!/bin/sh' 'echo rustc 1.98.1' > "$CARGO_HOME/bin/rustc"
chmod +x "$CARGO_HOME/bin/cargo" "$CARGO_HOME/bin/rustc"
(
    export PATH="$TEST_TMP/bin:/usr/bin:/bin"
    command_exists() { command -v "$1" >/dev/null 2>&1; }
    install_rust_secure
    [[ "$(command -v cargo)" == "$CARGO_HOME/bin/cargo" ]]
)

echo "install checksum tests passed"
