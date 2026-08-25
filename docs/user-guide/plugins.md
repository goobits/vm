# Plugins

Plugins extend configuration through reusable preset definitions. Service
manifests can also be installed, inspected, and validated without adding new
top-level commands.

Database and secret workflows are built-in command groups rather than plugin
commands:

```bash
vm db ls
vm secret interactive
```

A plugin does not claim an arbitrary command namespace. Managed guests receive
remote command namespaces through the separate controller-approved registry.
Use `vm plugin --help`, `vm db --help`, or `vm secret --help` for
installed-version help. The
[CLI Reference](cli-reference.md#plugins-databases-and-secrets) owns the
documented public inventory.
