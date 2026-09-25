# Plugins

Plugins extend configuration through reusable presets and declarative container
services. They do not add top-level commands.

`vm plugins validate PATH` checks a local source before installation.
`vm plugins install PATH` accepts a directory containing `plugin.yaml` and
exactly one matching `preset.yaml` or `service.yaml`; `README.md` is optional.
Installation rejects links, nested directories, extra files, unknown manifest
fields, invalid names, and conflicting installed names. It validates a staged
copy before making the plugin visible. Install only manifests you trust: a
preset can select images, packages, services, mounts, host sync, and tools when
applied. Plugin `provision` scripts and arbitrary command extensions are not
supported. Service plugins cannot override the image command, run health-check
commands, or mount host paths.

Activate an installed service plugin in a Docker or Podman project:

```yaml
provider: docker
services:
  cache:
    enabled: true
    plugin: redis-cache
```

The service key (`cache`) identifies the service in the project; Compose names
it `plugin_cache` and provides the `cache` network alias. Its installed
`service.yaml` supplies the image, optional `host:container` TCP ports, named
volumes, environment values, and dependencies on other active plugin service
keys (or `postgres` when built-in PostgreSQL is enabled). Service plugins are
isolated from host paths; their named volumes use instance-scoped VM ownership
labels and survive normal environment removal. The project must use distinct
host ports across service plugins, built-in services, and application mappings.
All manifests, dependencies, ports, and provider capabilities are checked
before creation. Installed but inactive plugins do not reserve ports.

Database and secret workflows are built-in command groups rather than plugin
commands:

```bash
vm db list
vm secrets set NAME
```

A plugin does not claim an arbitrary command namespace. Installed tools run as
guest executables on `PATH`, or through `vm exec -- TOOL ...` from the host.
Use `vm plugins --help`, `vm db --help`, or `vm secrets --help` for
installed-version help. The
[CLI Reference](cli-reference.md#plugins-databases-and-secrets) owns the
documented public inventory.
