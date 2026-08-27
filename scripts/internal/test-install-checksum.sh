#!/bin/bash

set -euo pipefail

TEST_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
TEST_TMP=$(mktemp -d)
trap 'rm -rf "$TEST_TMP"' EXIT

export HOME="$TEST_TMP/home"
mkdir -p "$HOME" "$TEST_TMP/bin"

cat > "$TEST_TMP/bin/curl" <<'EOF'
#!/bin/bash
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
fixture_hash=$(sha256sum "$fixture" | awk '{print $1}')

FAKE_CHECKSUM_RESPONSE="$fixture_hash  rustup-init"
export FAKE_CHECKSUM_RESPONSE
verify_rustup_checksum "$fixture"

expected_url="https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init"
if [[ "$(rustup_init_url)" != "$expected_url" ]]; then
    echo "rustup download URL does not identify the verified rustup-init artifact" >&2
    exit 1
fi

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

echo "install checksum tests passed"
