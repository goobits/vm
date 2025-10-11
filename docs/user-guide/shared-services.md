# Shared Database Services

The VM tool provides optional, shared database services that run on the host machine and can be accessed by any VM. This approach offers significant benefits over running a separate database instance inside each VM.

## Supported Services

- PostgreSQL
- Redis
- MongoDB

## Benefits

| Aspect             | Per-VM Database (Default) | Shared Service            |
| ------------------ | ------------------------- | ------------------------- |
| **Memory Usage**   | High (e.g., 200MB × N VMs) | Low (e.g., 200MB × 1)     |
| **Startup Time**   | Added delay to `vm create`  | ~5s once (for first VM)   |
| **Data Persistence** | Lost on `vm destroy`        | Persistent across VMs     |
| **Data Location**  | Scattered inside VMs      | Centralized in `~/.vm/data` |
| **Port Management**  | Potential for conflicts   | Single, managed port      |

## Enabling Shared Services

Shared services are **disabled by default**. You must opt-in to use them. You can enable each service individually using the `vm config set` command.

### Enable PostgreSQL

```bash
vm config set services.postgresql.enabled true
```

### Enable Redis

```bash
vm config set services.redis.enabled true
```

### Enable MongoDB

```bash
vm config set services.mongodb.enabled true
```

When you create a VM after enabling a service, the service will be automatically started if it's not already running. It will be automatically stopped when the last VM using it is destroyed.

## Connecting to Services

When a shared service is enabled, the VM tool automatically injects environment variables into your VM to make connecting easy.

- `DATABASE_URL` for PostgreSQL
- `REDIS_URL` for Redis
- `MONGODB_URL` for MongoDB

These URLs point to the service running on the host machine.

### Example: Connecting to PostgreSQL

Your application can read the `DATABASE_URL` environment variable to connect.

```
postgresql://postgres:postgres@host.docker.internal:5432/my-project
```

- **Host:** `host.docker.internal` (for Docker provider)
- **Port:** The configured port (default: 5432)
- **User/Password:** `postgres`/`postgres` (a simple default for local development)
- **Database:** The database name is automatically set to your project's name.

### Example: Connecting to Redis

Your application can use the `REDIS_URL`.

```
redis://host.docker.internal:6379
```

## Data Persistence and Location

All data for shared services is stored in your home directory under `~/.vm/data/`.

- **PostgreSQL:** `~/.vm/data/postgres`
- **Redis:** `~/.vm/data/redis`
- **MongoDB:** `~/.vm/data/mongodb`

This data **persists** even when you run `vm destroy`.

## Configuration

You can customize the port, version, and data directory for each service by editing the global configuration file at `~/.vm/config.yaml`.

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
```
