# Plugins

Plugins extend `vm` without expanding the built-in core.

```bash
vm plugin ls
vm plugin info <name>
vm plugin install <path>
vm plugin rm <name>
vm plugin new <name> --type <preset|service>
vm plugin validate <name>
```

Plugin-backed commands stay flat when available:

```bash
vm db ls
vm fleet ls
vm secret add API_KEY value
```

A plugin can provide presets, services, or command integrations while keeping the core CLI focused on everyday environment lifecycle.
