# Shared Database Services Proposal

## Problem Statement

Managing databases (Postgres, Redis, MongoDB) inside individual VMs is inefficient:
- Each VM starts its own database instance (~200MB memory per instance)
- Data is lost when VM is destroyed
- Slow startup times (5+ seconds per VM)
- Port conflicts when running multiple VMs
- Multiple copies of the same data across VMs

## Proposed Solution

Extend the existing `GlobalServices` system to support shared database services that run once on the host and are accessible to all VMs.

## Architecture

### Current State
The `vm` tool already has a `ServiceManager` (`rust/vm/src/service_manager.rs`) that manages:
- Docker Registry (port 5000)
- Auth Proxy (port 3090)
- Package Registry (port 3080)

Features:
- Reference counting (auto-start when first VM needs it, auto-stop when last VM stops)
- State persistence across CLI restarts
- Health checking
- Lifecycle management

### Proposed Extension

Add database services to the same system:
- PostgreSQL (default port 5432)
- Redis (default port 6379)
- MongoDB (default port 27017)

## Implementation

### 1. Configuration (`~/.vm/config.yaml`)

```yaml
services:
  postgresql:
    enabled: true
    port: 5432
    version: "16"
    data_dir: ~/.vm/data/postgres

  redis:
    enabled: true
    port: 6379
    version: "7"
    data_dir: ~/.vm/data/redis

  mongodb:
    enabled: true
    port: 27017
    version: "7"
    data_dir: ~/.vm/data/mongodb
```

### 2. Service Implementation

Each service runs as a Docker container managed by `ServiceManager`:

```bash
# PostgreSQL
docker run -d --name vm-postgres-global \
  -p 5432:5432 \
  -v ~/.vm/data/postgres:/var/lib/postgresql/data \
  postgres:16

# Redis
docker run -d --name vm-redis-global \
  -p 6379:6379 \
  -v ~/.vm/data/redis:/data \
  redis:7

# MongoDB
docker run -d --name vm-mongodb-global \
  -p 27017:27017 \
  -v ~/.vm/data/mongodb:/data/db \
  mongo:7
```

### 3. VM Connection

VMs connect to shared services via host networking:

**Docker Provider:**
```bash
# Inside VM
psql -h host.docker.internal -p 5432 -U postgres
redis-cli -h host.docker.internal -p 6379
mongosh --host host.docker.internal --port 27017
```

**Tart Provider (macOS):**
```bash
# Inside VM (host IP varies by VM software)
psql -h 192.168.65.2 -p 5432 -U postgres
```

### 4. Database Isolation

Each project automatically gets its own database:

```sql
-- Auto-created on first VM start
CREATE DATABASE "my-project";
GRANT ALL PRIVILEGES ON DATABASE "my-project" TO postgres;
```

Project config specifies connection:
```yaml
# vm.yaml
environment:
  DATABASE_URL: "postgresql://postgres@host.docker.internal:5432/my-project"
  REDIS_URL: "redis://host.docker.internal:6379"
```

## Benefits

| Aspect | Current (Per-VM) | Proposed (Shared) |
|--------|------------------|-------------------|
| Memory Usage | 200MB × N VMs | 200MB × 1 |
| Startup Time | ~5s per VM create | ~5s once (first VM) |
| Data Persistence | Lost on `vm destroy` | Persistent |
| Backup Location | N scattered locations | `~/.vm/data/` |
| Port Management | Potential conflicts | Single managed port |

## Code Changes Required

1. **`rust/vm-config/src/global_config.rs`** (~100 lines)
   - Add `PostgresSettings`, `RedisSettings`, `MongoDBSettings` structs
   - Update `GlobalServices` struct

2. **`rust/vm/src/service_manager.rs`** (~150 lines)
   - Add `start_postgres()`, `start_redis()`, `start_mongodb()` methods
   - Add health check implementations
   - Add container cleanup on stop

3. **`rust/vm-provider/src/docker/compose.rs`** (~50 lines)
   - Inject `DATABASE_URL` environment variables when shared services enabled

**Estimated Total:** ~300 lines

## Risks & Mitigations

### Risk 1: Version Conflicts
**Problem:** Different projects need different Postgres versions
**Mitigation:** Global config specifies one version. Projects adapt or run local instance if needed.

### Risk 2: Data Conflicts
**Problem:** Multiple projects accessing same database
**Mitigation:** Auto-provision separate database per project (`CREATE DATABASE project_name`)

### Risk 3: Docker Networking
**Problem:** Container-to-host networking may not work everywhere
**Mitigation:** Already solved - Docker provider uses `host.docker.internal`, Tart uses host IP

### Risk 4: Service Not Running
**Problem:** VM expects database but it's not started
**Mitigation:** ServiceManager auto-starts when first VM needs it (existing behavior)

## Opt-In Behavior

Services are **disabled by default**. Users must explicitly enable:

```bash
# Enable shared Postgres
vm config set services.postgresql.enabled true

# Or edit ~/.vm/config.yaml directly
```

## Migration Path

**Phase 1:** Implement Postgres only (most common use case)
**Phase 2:** Add Redis
**Phase 3:** Add MongoDB

Each phase is independent - services can be added incrementally.

## Alternative Considered

**Per-VM databases (current approach):**
- Pro: Project isolation is guaranteed
- Pro: No shared service complexity
- Con: Resource inefficient
- Con: Data doesn't persist across `vm destroy`

**Decision:** Shared services are optional and disabled by default, so users can choose based on their needs.

## Open Questions

1. Should shared services be enabled by default? (Proposal: No, opt-in)
2. How to handle database migrations? (Proposal: Leave to user, just provide connection)
3. Should we support multiple versions simultaneously? (Proposal: No, one version per service)
4. How to backup shared databases? (Proposal: Document pg_dump to `~/.vm/data/backups/`)

## Success Criteria

- [ ] User can enable shared Postgres via `vm config set services.postgresql.enabled true`
- [ ] ServiceManager auto-starts Postgres when first VM needs it
- [ ] VMs can connect via `host.docker.internal:5432`
- [ ] Data persists across `vm destroy && vm create`
- [ ] Service auto-stops when last VM is destroyed
- [ ] Health checks verify service is running

## Implementation Timeline

- **Week 1:** Add Postgres configuration and start/stop methods
- **Week 2:** Add database auto-provisioning per project
- **Week 3:** Add Redis and MongoDB using same pattern
- **Week 4:** Documentation and testing

## Documentation Needed

- `docs/user-guide/shared-services.md` - How to enable and use
- `README.md` - Add section on shared database services
- `CLAUDE.md` - Developer notes on ServiceManager architecture
