#!/bin/bash
set -euo pipefail

# Simulate CI environment
export CI=true
export TERM=dumb
export DEBIAN_FRONTEND=noninteractive
export VM_NO_PROMPT=true

echo "=== CI/CD Automated Onboarding Test ==="

# Step 1: Install Rust (if needed)
if ! command -v cargo &> /dev/null; then
    echo "Installing Rust toolchain..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    source "$HOME/.cargo/env"
fi

# Step 2: Build VM tool from source
echo "Building VM tool from source..."
(cd rust && time cargo build --package vm --release)

# Add the binary to the PATH and define a variable for the binary
export PATH="$PWD/.build/target/release:$PATH"
VM_BIN="$PWD/.build/target/release/vm"

# Verify installation
$VM_BIN --version || { echo "ERROR: VM not built or not in PATH"; exit 1; }

# Step 3: Create test project
echo "Creating test project..."
TEST_DIR=$(mktemp -d)
cd "$TEST_DIR"

cat > package.json <<'EOF'
{
  "name": "ci-test-app",
  "version": "1.0.0",
  "scripts": {
    "test": "echo 'Tests pass'"
  }
}
EOF

cat > index.js <<'EOF'
console.log('Hello from automated CI');
EOF

# Create a minimal vm.yaml to work around the non-interactive onboarding issue
cat > vm.yaml <<'EOF'
provider: docker
project:
  name: ci-test-app
EOF

# Step 4: Create VM (non-interactive)
echo "Creating VM..."
MAX_RETRIES=3
RETRY_DELAY=10
for ((i=1; i<=MAX_RETRIES; i++)); do
  if sudo -E "$VM_BIN" create --force; then
    break
  fi
  if [[ $i -lt $MAX_RETRIES ]]; then
    echo "vm create failed. Retrying in $RETRY_DELAY seconds..."
    sleep $RETRY_DELAY
  else
    echo "vm create failed after $MAX_RETRIES attempts."
    exit 1
  fi
done

# Step 5: Run tests
echo "Testing VM commands..."

# Test list
sudo -E "$VM_BIN" list

# Test status
sudo -E "$VM_BIN" status

# Test exec
sudo -E "$VM_BIN" exec "node --version"
sudo -E "$VM_BIN" exec "npm --version"
sudo -E "$VM_BIN" exec "cat package.json"
sudo -E "$VM_BIN" exec "npm test"

# Test SSH with command
sudo -E "$VM_BIN" ssh --command "echo 'CI can SSH'"
sudo -E "$VM_BIN" ssh --command "ls -la"
sudo -E "$VM_BIN" ssh --command "pwd"

# Step 6: Cleanup
echo "Cleaning up..."
sudo -E "$VM_BIN" destroy --force --all

# Verify cleanup
if sudo -E /usr/bin/docker ps -a --format '{{.Names}}' | grep -q 'ci-test'; then
    echo "ERROR: Cleanup failed, containers still exist"
    exit 1
fi

echo "=== All CI/CD tests passed ==="
