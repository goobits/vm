# Shared Services

Shared services provide durable databases and infrastructure for environments.

Common workflows:

```bash
vm start app
vm db list
vm db backups create daily --all
vm db credentials postgresql
```

Service data is managed separately from active environment resources. `vm remove <name>` removes the active environment and preserves explicitly saved snapshots.

Use configuration to enable services and ports, then run the environment:

```bash
vm config show
vm start app
```

Database commands operate on the selected project's configured PostgreSQL database. For named environments, add a complete `services.postgresql` definition under each `environments.<name>` declaration when they need separate database identities. Select one with `--env <name>`; its backups live in a separate project and environment directory.
