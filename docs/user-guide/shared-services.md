# Shared Services

Shared services provide durable databases and infrastructure for environments.

Common workflows:

```bash
vm run linux as app
vm db ls
vm db backup app_db
vm db credentials postgresql
```

Service data is managed separately from active environment resources. `vm remove <name>` removes the active environment and preserves explicitly saved snapshots.

Use configuration to enable services and ports, then run the environment:

```bash
vm config show
vm run linux as app
```
