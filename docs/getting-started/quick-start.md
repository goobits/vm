# Quick Start

Initialize a project, declare an environment, and start it:

```bash
vm init
vm create dev --provider docker --image ubuntu:24.04
vm start dev
```

`create` records and provisions a stopped environment. `start` runs it. If a
configured environment is missing at runtime, `start` provisions it from its
declaration.

## Work Inside It

```bash
vm shell dev
vm exec --env dev -- npm test
vm logs dev --follow
vm copy --env dev host:./config.json env:/workspace/config.json
```

`shell` and `exec` require a running environment. Omit the name when the project
has an unambiguous default environment.

## See What Is Running

```bash
vm list
vm status dev
```

`vm list --all-projects` shows registered environments across projects.

## Stop Or Remove It

```bash
vm stop dev
vm restart dev
vm remove dev
```

Removal preserves persistent data and snapshots unless you explicitly request
owned data removal.

## Choose A Provider

```bash
vm create backend --provider docker --image ubuntu:24.04
vm create isolated --provider tart --image vibe-tart-linux-base
vm create db --provider podman --image postgres:17
```

Use a compatible local or published image for each provider. A Tart macOS guest
requires Apple Silicon macOS and a macOS base image.

## Save And Restore State

```bash
vm snapshots create stable --env dev
vm snapshots restore stable --env dev
vm snapshots export stable --output dev.tar.gz
```

## Advanced Tools

```bash
vm config show
vm tunnels open web --local 127.0.0.1:8080 --remote 127.0.0.1:3000 --env dev
vm doctor
vm system update
```

Database and secret workflows stay top-level:

```bash
vm db list
vm secrets set NAME
```
