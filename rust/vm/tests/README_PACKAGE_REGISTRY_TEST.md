# Package Registry Integration Test

## Overview

The `test_package_registry_feature()` integration test validates that the package registry configuration works end-to-end in a live Docker environment.

## What It Tests

This test creates a real VM with package registry enabled and verifies:

1. ✅ **Environment Variables**: All 6 registry env vars are injected into the container
   - `NPM_CONFIG_REGISTRY`
   - `PIP_INDEX_URL`
   - `PIP_EXTRA_INDEX_URL`
   - `PIP_TRUSTED_HOST`
   - `VM_CARGO_REGISTRY_HOST`
   - `VM_CARGO_REGISTRY_PORT`

2. ✅ **Cargo Configuration**: `~/.cargo/config.toml` is created with correct registry URL

3. ✅ **Platform Detection**: Uses correct host gateway based on OS (Linux vs macOS/Windows)

4. ✅ **VM Lifecycle**: VM is created and destroyed cleanly

## Test Location

File: `/workspace/rust/vm/tests/vm_operations_integration_tests.rs`

Function: `test_package_registry_feature()`

## Running The Test

### Prerequisites

- Docker daemon running
- VM binary built (`cargo build`)

### Run With Docker Available

```bash
# Run only this test
cargo test --package vm --test vm_operations_integration_tests test_package_registry_feature -- --nocapture --test-threads=1

# Run all VM operations integration tests
cargo test --package vm --test vm_operations_integration_tests -- --test-threads=1
```

### Run Without Docker (Skip Mode)

If Docker is not available, the test will automatically skip with a warning:

```
⚠️  Skipping test_package_registry_feature - Docker not available
```

This is intentional - integration tests should not fail on systems without Docker.

## Test Flow

1. **Setup**: Creates isolated temp directory with unique project name
2. **Global Config**: Creates `~/.vm/config.yaml` with `package_registry.enabled: true`
3. **VM Creation**: Runs `vm create` command
4. **Env Var Verification**: Execs into container and runs `printenv`
5. **Cargo Config Verification**: Sources `.zshrc` and checks `~/.cargo/config.toml`
6. **Cleanup**: Runs `vm destroy --force` and verifies container is gone

## Expected Output (When Docker Available)

```
Creating VM with package registry enabled...
Verifying environment variables in container...
Verifying Cargo configuration file...
✅ Cargo configuration verified successfully
Cleaning up test VM...
✅ Package registry integration test completed successfully
```

## Troubleshooting

### Test Fails: "Container not found"

The container wasn't created successfully. Check:
- Docker daemon is running
- No conflicting containers with same name
- Sufficient disk space for Docker images

### Test Fails: "Environment variable not set"

The registry env vars weren't injected. This indicates a bug in:
- `vm-provider/src/docker/compose.rs` (env var injection logic)
- `vm-provider/src/context.rs` (context passing)
- `vm/src/commands/vm_ops.rs` (context creation)

### Test Fails: "Cargo config not found"

The `.zshrc` script didn't create `~/.cargo/config.toml`. Check:
- `vm-provider/src/docker/Dockerfile.j2` (shell init script)
- Container uses zsh as default shell

### Test Times Out

Container creation is taking too long. Increase the sleep duration:

```rust
std::thread::sleep(Duration::from_secs(10)); // Increase from 3
```

## Verification With Real Docker

To manually verify the test behavior:

```bash
# 1. Create global config
mkdir -p ~/.vm
cat > ~/.vm/config.yaml << 'EOF'
services:
  package_registry:
    enabled: true
    port: 3080
EOF

# 2. Create test VM
cd /tmp/test-project
cat > vm.yaml << 'EOF'
provider: docker
project:
  name: registry-test
vm:
  memory: 1024
  cpus: 1
EOF

vm create

# 3. Check env vars
docker exec registry-test-dev printenv | grep -E "(NPM_CONFIG|PIP_|VM_CARGO)"

# 4. Check cargo config
docker exec registry-test-dev /bin/zsh -l -c "cat ~/.cargo/config.toml"

# 5. Cleanup
vm destroy --force
```

## Related Tests

- **Unit Tests**: `vm-provider/src/docker/compose.rs::tests`
  - `test_package_registry_env_vars_injection`
  - `test_start_with_compose_regenerates_with_new_config`

- **CLI Tests**: `vm/tests/pkg_cli_tests.rs`
  - Tests `vm pkg registry` commands

- **Integration Tests**: This file
  - `test_package_registry_feature` (Phase 2 validation)

## Success Criteria

✅ Test passes when Docker is available
✅ Test skips gracefully when Docker is not available
✅ All 6 environment variables are correctly set in the container
✅ Cargo config file is created with correct registry URL
✅ Container is cleanly destroyed after test
✅ All existing integration tests continue to pass
