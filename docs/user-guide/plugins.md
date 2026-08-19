# Plugins

Plugins extend `vm` without expanding the built-in core.

Plugin-backed commands stay flat when available:

```bash
vm db ls
vm secret interactive
```

A plugin can provide presets, services, or command integrations while keeping
the core CLI focused on everyday environment lifecycle. Use `vm plugin --help`,
`vm db --help`, or `vm secret --help` for installed-version help. The
[CLI Reference](cli-reference.md#plugins-databases-and-secrets) owns the
documented public inventory.
