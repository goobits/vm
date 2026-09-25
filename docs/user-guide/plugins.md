# Plugins

Plugins extend configuration through reusable preset definitions. Service
manifests can also be installed, inspected, and validated without adding new
top-level commands.

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
