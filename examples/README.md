# VM Configuration Examples

This directory contains example VM configurations to help you get started.

## Directory Structure

- `configurations/` - Complete VM configuration examples
  - `minimal.yaml` - Bare minimum configuration
  - `full-stack.yaml` - Comprehensive multi-service setup

- `services/` - Individual service configuration examples
  - `postgresql.yaml` - PostgreSQL database setup
  - `redis.yaml` - Redis cache setup
  - `mongodb.yaml` - MongoDB database setup

## Usage

Copy any example file to your project directory and customize it:

```bash
# Copy minimal example
cp examples/configurations/minimal.yaml vm.yaml

# Or copy a service-specific example
cp examples/services/postgresql.yaml vm.yaml
```

Then edit the file to match your project's needs.

## Learn More

See the [Configuration Guide](../docs/user-guide/configuration.md) for detailed documentation.