# Quick Start

`vm` creates humane virtual environments from intent-first commands.

## Start An Environment

```bash
vm run linux as dev
```

This creates the environment if needed, starts it, and names it `dev`.

## Work Inside It

```bash
vm shell dev
vm exec dev -- npm test
vm logs dev --follow
vm copy ./config.json dev:/workspace/config.json
```

## See What Is Running

```bash
vm ls
```

This lists environments for the current project. Use `vm ls --all` to see every `vm` environment on the machine.

## Stop Or Remove It

```bash
vm stop dev
vm rm dev
```

Removing an environment frees active resources. Saved snapshots are preserved.

## Pick A Kind

```bash
vm run mac as xcode
vm run linux as backend
vm run container as redis
```

Unnamed environments can be addressed by kind:

```bash
vm run mac
vm shell mac
```

The default routing is:

| Kind | Default engine |
| --- | --- |
| `mac` | Tart |
| `linux` | Docker |
| `container` | Docker |

Override routing only when needed:

```bash
vm run linux as secure --provider tart
vm run container as db --provider podman
```

## Save And Restore State

```bash
vm save dev as stable
vm revert dev stable
vm package dev --output dev.tar.gz
```

## Advanced Tools

```bash
vm config show
vm tunnel add 8080:3000 dev
vm doctor
vm system update
```

Plugin-backed workflows stay top-level:

```bash
vm db ls
vm fleet ls
vm secret add API_KEY value
```
