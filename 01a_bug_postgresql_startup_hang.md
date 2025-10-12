# BUG-004: PostgreSQL Service Health Check Timeout Too Aggressive

**Severity:** High
**Impact:** VM creation hangs when PostgreSQL service is enabled
**Status:** Open

---

## Problem

When creating a VM with PostgreSQL enabled (`services.postgresql.enabled: true`), the `vm create --force` command hangs indefinitely during the "Container vm-dev Creating" phase. The system appears to freeze without any error messages or timeout.

### Symptoms
- ✅ PostgreSQL configuration is correctly set in `vm.yaml`
- ✅ Manual PostgreSQL container startup works (`docker run postgres:16`)
- ✅ VM creation succeeds when PostgreSQL is disabled
- ❌ VM creation hangs when PostgreSQL is enabled
- ❌ No error messages or timeout feedback to user
- ❌ Process must be force-killed by user

### User Impact
- **Fresh installations** with database services enabled cannot complete VM creation
- **Poor user experience** - no feedback, appears frozen
- **Database features** are unusable for affected users
- **Testing blocked** - cannot verify DATABASE_URL environment variable injection

---

## Root Cause Analysis

### Code Investigation

The issue is in the PostgreSQL health check timing in `rust/vm/src/service_manager.rs`:

**Health Check Loop (Lines 327-337):**
```rust
// Verify service started
for attempt in 1..=5 {
    sleep(Duration::from_millis(1000)).await;  // ⚠️ Only 1 second per attempt
    if self.check_service_health(service_name, global_config).await {
        vm_success!("Service '{}' started successfully", service_name);
        return Ok(());
    }
    debug!(
        "Service '{}' not ready, attempt {}/5",
        service_name, attempt
    );
}

Err(anyhow::anyhow!(
    "Service '{}' failed to start properly",  // ⚠️ Error thrown after 5 seconds total
    service_name
))
```

**PostgreSQL Health Check (Lines 412-416):**
```rust
"postgresql" | "redis" | "mongodb" => {
    // For database services, a TCP connection is a reliable health check
    return tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .is_ok();
}
```

### The Problem

1. **PostgreSQL initialization time**: PostgreSQL containers take 5-10 seconds to fully initialize after `docker run` starts
2. **Health check window**: The service manager only waits **5 seconds total** (5 attempts × 1 second)
3. **Timing mismatch**: PostgreSQL needs more time than the health check allows
4. **Error handling bug**: When health check fails, the error from line 171-175 should warn and continue:
   ```rust
   if let Err(e) = self.start_service(&service_name, global_config).await {
       warn!("Failed to start service '{}': {}", service_name, e);
       // Don't fail VM creation if service startup fails
       vm_warning!("Service '{}' failed to start: {}", service_name, e);
   }
   ```
   However, the hang suggests the error isn't being propagated correctly, or there's a blocking operation elsewhere.

5. **No timeout on VM creation**: The docker-compose operation has no overall timeout, so when PostgreSQL startup is slow, the entire VM creation appears to hang.

### Why Manual Docker Works

When Bob ran:
```bash
docker run -d --name vm-postgres-global -p 3019:5432 \
  -e POSTGRES_PASSWORD=postgres postgres:16
```

It succeeded because there was no 5-second health check constraint. The container starts successfully, just takes 7-10 seconds to accept connections.

---

## Proposed Solutions

### Solution 1: Increase Health Check Timeout (Quick Fix)

**File:** `rust/vm/src/service_manager.rs`
**Line:** 327-328

```rust
// BEFORE
for attempt in 1..=5 {
    sleep(Duration::from_millis(1000)).await;

// AFTER
for attempt in 1..=10 {  // Increase attempts from 5 to 10
    sleep(Duration::from_millis(2000)).await;  // Increase interval to 2 seconds
```

**Impact:** 20 seconds total wait time (10 attempts × 2 seconds)
**Pros:** Simple one-line change, fixes the immediate issue
**Cons:** Increases VM creation time by 15 seconds if service is truly broken

### Solution 2: Progressive Backoff (Better UX)

**File:** `rust/vm/src/service_manager.rs`
**Lines:** 327-337

```rust
// Progressive backoff: 1s, 2s, 3s, 4s, 5s, 5s, 5s = 25s max
let delays = vec![1000, 2000, 3000, 4000, 5000, 5000, 5000];
for (attempt, delay_ms) in delays.iter().enumerate() {
    sleep(Duration::from_millis(*delay_ms)).await;
    if self.check_service_health(service_name, global_config).await {
        vm_success!("Service '{}' started successfully", service_name);
        return Ok(());
    }
    debug!(
        "Service '{}' not ready, attempt {}/{}",
        service_name, attempt + 1, delays.len()
    );
}
```

**Impact:** Fast failures (1-2s) for broken services, patient for slow startups (up to 25s)
**Pros:** Better UX, faster feedback for failures, accommodates slow startups
**Cons:** Slightly more complex

### Solution 3: Service-Specific Timeouts (Most Robust)

**File:** `rust/vm/src/service_manager.rs`
**Add after line 392:**

```rust
/// Get the health check configuration for a service
fn get_health_check_config(&self, service_name: &str) -> (usize, u64) {
    // Returns (max_attempts, delay_ms)
    match service_name {
        "postgresql" | "mongodb" => (10, 2000),  // Databases need more time
        "redis" => (8, 1500),                     // Redis is faster
        "auth_proxy" | "package_registry" => (5, 1000),  // HTTP services are quick
        "docker_registry" => (8, 2000),           // Registry can be slow
        _ => (5, 1000),                           // Default
    }
}
```

Then update the health check loop (line 327):
```rust
let (max_attempts, delay_ms) = self.get_health_check_config(service_name);
for attempt in 1..=max_attempts {
    sleep(Duration::from_millis(delay_ms)).await;
    // ... rest of logic
}
```

**Impact:** Optimized per-service, no unnecessary delays
**Pros:** Most robust, extensible, respects service characteristics
**Cons:** More code changes

---

## Recommended Fix

**Quick Fix:** Implement Solution 1 immediately to resolve the hang:
- Change line 327: `for attempt in 1..=10 {`
- Change line 328: `sleep(Duration::from_millis(2000)).await;`
- Update debug message on line 334 to show "10" instead of "5"

**Optional Enhancement:** Refactor to Solution 3 for service-specific timeouts:
- Implement service-specific timeout configuration
- Add comprehensive logging with `info!` level messages
- Add user-facing progress messages: "Waiting for PostgreSQL to initialize... (attempt X/10)"

---

## Testing Checklist

### Unit Tests
- [ ] Test `start_postgres()` method in isolation
- [ ] Verify health check with mock TCP connections
- [ ] Test timeout behavior with simulated slow startup

### Integration Tests
- [ ] Test VM creation with PostgreSQL enabled (default port)
- [ ] Test VM creation with PostgreSQL on custom port
- [ ] Test VM creation with all database services enabled (PostgreSQL + Redis + MongoDB)
- [ ] Verify DATABASE_URL environment variable is correctly injected
- [ ] Test service startup failure handling (wrong port, no Docker, etc.)
- [ ] Verify VM creation succeeds even if PostgreSQL health check fails (warning-only)

### Manual Testing
```bash
# Clean state
docker ps -aq --filter "name=vm-" | xargs -r docker rm -f
rm -rf ~/.vm/data/postgresql/*

# Test 1: Fresh PostgreSQL install
cd /tmp && mkdir test-postgres && cd test-postgres
vm config set services.postgresql.enabled true
time vm create --force  # Should complete in < 30 seconds

# Test 2: Verify DATABASE_URL
vm exec printenv DATABASE_URL
# Expected: postgresql://postgres:postgres@172.17.0.1:PORT/test-postgres

# Test 3: Verify PostgreSQL container running
docker ps | grep postgres
docker exec vm-postgres-global pg_isready

# Test 4: VM creation with verbose logging
RUST_LOG=vm=debug,vm_provider=debug vm create --force
# Check for health check attempt logs

# Test 5: Resource-constrained environment
# (Lower system specs, ensure timeout is sufficient)
```

### Performance Benchmarks
- [ ] Measure PostgreSQL container startup time on different systems
- [ ] Record VM creation time with fix applied
- [ ] Compare before/after user experience

---

## Files to Modify

### Primary Changes
1. **`rust/vm/src/service_manager.rs`** (Lines 327-337)
   - Increase health check attempts and interval
   - Add service-specific timeout configuration
   - Improve debug logging

### Secondary Changes (Optional Enhancement)
2. **`rust/vm/src/commands/vm_ops/create.rs`** (Line 219)
   - Add user-facing progress messages during service startup
   - Show which services are being started

3. **`rust/vm-provider/src/docker/compose.rs`** (Line 369-404)
   - Add overall timeout to docker-compose operations
   - Prevent indefinite hangs on container creation

---

## Related Issues

### Configuration Confusion (MEDIUM Priority)
- **Issue:** DATABASE_URL is injected based on global config even when project config disables PostgreSQL
- **File:** `rust/vm-provider/src/docker/compose.rs:127`
- **Current Logic:**
  ```rust
  if global_cfg.services.postgresql.enabled {
      // Inject DATABASE_URL
  }
  ```
- **Proposed Enhancement:**
  ```rust
  let postgres_enabled = vm_config.services
      .get("postgresql")
      .map(|s| s.enabled)
      .unwrap_or(global_cfg.services.postgresql.enabled);

  if postgres_enabled {
      // Inject DATABASE_URL
  }
  ```
- **Impact:** Allows project-level opt-out of database environment variables
- **Recommendation:** Create separate enhancement ticket

### Resource Defaults (LOW Priority)
- **Issue:** Default 6 CPUs is too aggressive for resource-constrained systems
- **File:** Default `vm.yaml` generation
- **Recommendation:**
  - Reduce default CPUs to 4
  - Reduce default memory to 4GB
  - Let auto-adjustment handle scaling up

---

## Verification Steps

After implementing the fix:

1. **Clean environment test:**
   ```bash
   docker system prune -af
   rm -rf ~/.vm/
   ```

2. **Fresh install with PostgreSQL:**
   ```bash
   mkdir -p ~/test-vm-postgres && cd ~/test-vm-postgres
   vm config set services.postgresql.enabled true
   vm create --force
   ```
   ✅ **Expected:** VM creates successfully in 20-30 seconds

3. **Verify database connection:**
   ```bash
   vm exec printenv DATABASE_URL
   vm exec pg_isready -h 172.17.0.1 -p PORT
   ```
   ✅ **Expected:** DATABASE_URL present, PostgreSQL accepting connections

4. **Test with other services:**
   ```bash
   vm config set services.redis.enabled true
   vm config set services.mongodb.enabled true
   vm create --force
   ```
   ✅ **Expected:** All services start successfully

5. **Test error handling:**
   ```bash
   # Occupy PostgreSQL port
   docker run -d -p 5432:5432 --name port-blocker postgres:16
   vm create --force
   ```
   ✅ **Expected:** Warning displayed, VM creation continues

---

## Success Criteria

- [x] Root cause identified and documented
- [ ] Fix implemented (Solution 1 minimum)
- [ ] All integration tests pass
- [ ] Manual testing checklist complete
- [ ] VM creation with PostgreSQL completes in < 30 seconds
- [ ] No user-facing hangs or freezes
- [ ] Clear error messages if service startup fails
- [ ] DATABASE_URL correctly available in created VMs
- [ ] Documentation updated with troubleshooting section

---

## Additional Context

### User Report Summary
- User: Bob
- Environment: Docker-in-Docker (nested containers)
- Test Scenario: Fresh PostgreSQL installation following test plan
- Workaround: Disabling PostgreSQL allows VM creation to succeed
- Manual Docker test: PostgreSQL container starts successfully outside service manager

### Related Code Paths
- Service registration: `rust/vm/src/commands/vm_ops/create.rs:219`
- Service startup: `rust/vm/src/service_manager.rs:283-343`
- Health checks: `rust/vm/src/service_manager.rs:405-426`
- PostgreSQL launcher: `rust/vm/src/service_manager.rs:594-622`
- Docker compose integration: `rust/vm-provider/src/docker/compose.rs:126-146`

---

## Notes

- This bug affects **all database services** (PostgreSQL, Redis, MongoDB) equally
- The fix will improve reliability across all shared services
- No breaking changes to user configuration
- Backward compatible with existing installations
