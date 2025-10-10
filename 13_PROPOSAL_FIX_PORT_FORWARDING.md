# Proposal: Fix Port Forwarding Configuration

**Status:** 🔴 Critical Bug
**Priority:** P0 (Blocker)
**Complexity:** Medium
**Estimated Effort:** 2-3 days

---

## Problem Statement

The port forwarding configuration specified in `vm.yaml` is completely ignored during VM creation. Only the automatically allocated port range is applied, making it impossible to access services running inside the VM on predictable ports.

### Impact

This bug prevents essential development workflows:
- ❌ **Web development** - Cannot access web servers (React, Express, Flask) on standard ports
- ❌ **API development** - Cannot test APIs on expected ports (e.g., 3000, 8080)
- ❌ **Database access** - Cannot connect to databases running in VMs
- ❌ **Microservices** - Cannot run multi-service architectures with specific port mappings
- ❌ **Documentation examples** - "Access your app at localhost:3000" doesn't work

**Result:** Port forwarding E2E test failed completely.

### Current Behavior

**Configuration in `vm.yaml`:**
```yaml
ports:
  - host: 3000
    guest: 3000
  - host: 8080
    guest: 8080
```

**Result after `vm create`:**
```bash
$ docker ps
CONTAINER ID   PORTS
abc123         0.0.0.0:3120-3129->3120-3129/tcp
# Only the auto-allocated range is mapped, NOT 3000 or 8080
```

**Attempting to access service:**
```bash
$ curl localhost:3000
curl: (7) Failed to connect to localhost port 3000: Connection refused
# Service is running in VM but not accessible
```

### Expected Behavior

**Configuration in `vm.yaml`:**
```yaml
ports:
  - host: 3000
    guest: 3000
```

**Result after `vm create`:**
```bash
$ docker ps
CONTAINER ID   PORTS
abc123         0.0.0.0:3000->3000/tcp, 0.0.0.0:3120-3129->3120-3129/tcp
# Both explicit ports AND auto-range are mapped
```

**Accessing service:**
```bash
$ curl localhost:3000
Hello from the VM!
```

---

## Root Cause Analysis

The port forwarding logic in the Docker provider is not reading or applying the `ports` configuration from `vm.yaml`.

### Likely Code Location

- `rust/vm-config/src/lib.rs` - Port configuration parsing
- `rust/vm-provider/src/docker/mod.rs` - Docker container creation
- `rust/vm-provider/src/docker/ports.rs` - Port mapping logic (if exists)

### Investigation Needed

1. ✅ Verify `vm.yaml` ports are being parsed correctly
2. ❌ Check if parsed ports are passed to the Docker provider
3. ❌ Verify Docker provider builds port mappings from config
4. ❌ Ensure port mappings are applied during `docker run`

---

## Proposed Solution

### 1. Architecture Overview

```
vm.yaml → Config Parser → Port Validator → Docker Provider → docker run -p
          ↓
        ports:
          - host: 3000
            guest: 3000
```

### 2. Implementation Plan

#### Step 1: Verify Config Parsing

**File:** `rust/vm-config/src/lib.rs`

Ensure port configuration is correctly parsed:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub host: u16,
    pub guest: u16,
    #[serde(default)]
    pub protocol: Protocol, // tcp or udp
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Tcp,
    Udp,
}

impl Default for Protocol {
    fn default() -> Self {
        Self::Tcp
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmConfig {
    // ... existing fields
    #[serde(default)]
    pub ports: Vec<PortMapping>,
    // ... existing fields
}
```

#### Step 2: Update Docker Provider to Accept Port Config

**File:** `rust/vm-provider/src/docker/mod.rs`

```rust
pub struct DockerProvider {
    // ... existing fields
}

impl Provider for DockerProvider {
    fn create(&self, config: &VmConfig) -> Result<()> {
        // ... existing code

        // Build port mappings
        let mut port_args = Vec::new();

        // 1. Add explicit port mappings from config
        for port_mapping in &config.ports {
            port_args.push("-p".to_string());
            port_args.push(format!(
                "{}:{}:{}",
                config.vm.port_binding, // e.g., "0.0.0.0"
                port_mapping.host,
                port_mapping.guest
            ));
        }

        // 2. Add auto-allocated port range (if configured)
        if let Some(port_range) = &config.ports_range {
            port_args.push("-p".to_string());
            port_args.push(format!(
                "{}:{}-{}:{}-{}",
                config.vm.port_binding,
                port_range.start,
                port_range.end,
                port_range.start,
                port_range.end
            ));
        }

        // Build docker run command
        let mut docker_args = vec![
            "run",
            "-d",
            "--name", &container_name,
        ];
        docker_args.extend(port_args.iter().map(|s| s.as_str()));
        // ... rest of docker run args

        Command::new("docker")
            .args(&docker_args)
            .status()?;

        Ok(())
    }
}
```

#### Step 3: Add Port Conflict Validation

**File:** `rust/vm-config/src/validator.rs`

```rust
pub fn validate_ports(config: &VmConfig) -> Result<()> {
    let mut used_host_ports = HashSet::new();

    for port in &config.ports {
        // Check for duplicate host ports
        if !used_host_ports.insert(port.host) {
            bail!("Duplicate host port mapping: {}", port.host);
        }

        // Validate port range
        if port.host == 0 || port.guest == 0 {
            bail!("Port numbers must be greater than 0");
        }
        if port.host > 65535 || port.guest > 65535 {
            bail!("Port numbers must not exceed 65535");
        }

        // Warn if using privileged ports
        if port.host < 1024 {
            warn!("Host port {} requires root/admin privileges", port.host);
        }
    }

    // Check for conflicts with auto-allocated range
    if let Some(range) = &config.ports_range {
        for port in &config.ports {
            if port.guest >= range.start && port.guest <= range.end {
                warn!(
                    "Guest port {} conflicts with auto-allocated range {}-{}",
                    port.guest, range.start, range.end
                );
            }
        }
    }

    Ok(())
}
```

#### Step 4: Add Port Availability Check

```rust
use std::net::TcpListener;

fn check_port_available(port: u16, binding: &str) -> Result<()> {
    let addr = format!("{}:{}", binding, port);
    match TcpListener::bind(&addr) {
        Ok(_) => Ok(()),
        Err(e) => {
            if e.kind() == std::io::ErrorKind::AddrInUse {
                bail!("Port {} is already in use on host", port);
            }
            Err(e.into())
        }
    }
}

pub fn validate_ports(config: &VmConfig) -> Result<()> {
    // ... existing validation

    // Check host port availability
    for port in &config.ports {
        check_port_available(port.host, &config.vm.port_binding)?;
    }

    Ok(())
}
```

### 3. Testing Strategy

#### Unit Tests

**File:** `rust/vm-config/src/tests.rs`

```rust
#[test]
fn test_port_config_parsing() {
    let yaml = r#"
ports:
  - host: 3000
    guest: 3000
  - host: 8080
    guest: 8080
    protocol: tcp
"#;
    let config: VmConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.ports.len(), 2);
    assert_eq!(config.ports[0].host, 3000);
    assert_eq!(config.ports[0].guest, 3000);
}

#[test]
fn test_port_validation_duplicate() {
    let config = VmConfig {
        ports: vec![
            PortMapping { host: 3000, guest: 3000, protocol: Protocol::Tcp },
            PortMapping { host: 3000, guest: 8080, protocol: Protocol::Tcp },
        ],
        ..Default::default()
    };
    assert!(validate_ports(&config).is_err());
}

#[test]
fn test_port_validation_invalid_range() {
    let config = VmConfig {
        ports: vec![
            PortMapping { host: 0, guest: 3000, protocol: Protocol::Tcp },
        ],
        ..Default::default()
    };
    assert!(validate_ports(&config).is_err());
}
```

#### Integration Tests

**File:** `rust/vm/tests/port_forwarding_tests.rs`

```rust
#[test]
fn test_port_forwarding_single_port() -> Result<()> {
    let temp_dir = create_temp_project()?;

    // Create vm.yaml with port mapping
    let config = r#"
version: 1.2.1
provider: docker
vm:
  cpus: 2
  memory: 2048
ports:
  - host: 3456
    guest: 3000
"#;
    std::fs::write(temp_dir.join("vm.yaml"), config)?;

    // Create and start VM
    run_vm_command(&["create"], &temp_dir)?;
    run_vm_command(&["start"], &temp_dir)?;

    // Verify port mapping exists
    let output = Command::new("docker")
        .args(&["port", "test-project-dev", "3000"])
        .output()?;

    let port_info = String::from_utf8(output.stdout)?;
    assert!(port_info.contains("3456"), "Port 3456 should be mapped");

    cleanup_vm(&temp_dir)?;
    Ok(())
}

#[test]
fn test_port_forwarding_multiple_ports() -> Result<()> {
    // Test multiple port mappings
    // Similar to above but with multiple ports
}

#[test]
fn test_port_conflict_detection() -> Result<()> {
    let temp_dir = create_temp_project()?;

    // Start a temporary server on port 3333
    let _listener = TcpListener::bind("127.0.0.1:3333")?;

    // Try to create VM with conflicting port
    let config = r#"
ports:
  - host: 3333
    guest: 3000
"#;
    std::fs::write(temp_dir.join("vm.yaml"), config)?;

    // Should fail with clear error
    let result = run_vm_command(&["create"], &temp_dir);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("already in use"));

    Ok(())
}
```

---

## Edge Cases to Handle

1. **Port conflicts:**
   - Host port already in use by another service
   - Multiple VMs trying to use same host port
   - Port in use by another container

2. **Privileged ports (1-1023):**
   - Require root/admin on Linux/Mac
   - Show clear error message with sudo suggestion

3. **Port range overlaps:**
   - Explicit port overlaps with auto-allocated range
   - Warn but allow (explicit takes precedence)

4. **Dynamic port allocation:**
   - Support `host: 0` for "any available port"
   - Report actual allocated port after creation

5. **Protocol support:**
   - Default to TCP
   - Support UDP when specified
   - Support both: `protocol: tcp+udp`

---

## Configuration Examples

### Basic Web Server
```yaml
ports:
  - host: 3000
    guest: 3000  # Node.js/React dev server
```

### Multi-Service Application
```yaml
ports:
  - host: 3000
    guest: 3000  # Frontend
  - host: 8080
    guest: 8080  # Backend API
  - host: 5432
    guest: 5432  # PostgreSQL
```

### UDP Protocol
```yaml
ports:
  - host: 5353
    guest: 5353
    protocol: udp  # mDNS
```

### Dynamic Allocation
```yaml
ports:
  - host: 0      # Pick any available port
    guest: 3000
  # Query with: vm port 3000
```

---

## Acceptance Criteria

- [ ] Explicit port mappings from `vm.yaml` are applied during `docker run`
- [ ] Both explicit ports and auto-range are mapped simultaneously
- [ ] Port conflict detection prevents creation with clear error
- [ ] Validation catches duplicate host port mappings
- [ ] Works with TCP (default) and UDP protocols
- [ ] Privileged ports show helpful error/warning
- [ ] `docker ps` shows all configured port mappings
- [ ] Services are accessible on host at configured ports
- [ ] Unit tests for config parsing and validation
- [ ] Integration tests for actual port forwarding
- [ ] E2E Port Forwarding test scenario passes

---

## Documentation Updates

### `vm.yaml` Reference

```yaml
# Port Forwarding Configuration
ports:
  # Map host port 3000 to guest port 3000 (TCP)
  - host: 3000
    guest: 3000

  # Map host port 8080 to guest port 80
  - host: 8080
    guest: 80

  # UDP protocol example
  - host: 5353
    guest: 5353
    protocol: udp

  # Auto-allocate host port (use 'vm port' to query)
  - host: 0
    guest: 3000

# Auto-allocated port range (for SSH, etc.)
ports:
  _range: [3120, 3129]
```

### CLI Help

Add new command:
```bash
$ vm port --help
Query port mappings for a VM

Usage:
  vm port <GUEST_PORT>     Show host port for guest port
  vm port --list           List all port mappings

Examples:
  vm port 3000            # Show which host port maps to guest 3000
  vm port --list          # Show all port mappings
```

---

## Timeline

- **Day 1:** Update config parsing, add validation, unit tests
- **Day 2:** Update Docker provider to apply port mappings, integration tests
- **Day 3:** Port query command, documentation, E2E validation

---

## Success Metrics

- ✅ E2E Port Forwarding test passes
- ✅ Web servers accessible on configured ports
- ✅ Port conflicts detected with clear errors
- ✅ No regression in auto-allocated port range
- ✅ Documentation includes working examples
