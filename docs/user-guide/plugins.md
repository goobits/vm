# Plugins

Plugins extend configuration through reusable preset definitions. Service
manifests can also be installed, inspected, and validated without adding new
top-level commands.

`vm plugins validate PATH` checks a local source before installation.
`vm plugins install PATH` accepts a directory containing `plugin.yaml` and
exactly one matching `preset.yaml` or `service.yaml`; `README.md` is optional.
Installation rejects links, nested directories, extra files, unknown manifest
fields, invalid names, and conflicting installed names. It validates a staged
copy before making the plugin visible. Install only manifests you trust: a
preset can select images, packages, services, mounts, host sync, and tools when
applied. Plugin `provision` scripts and arbitrary command extensions are not
supported. Service manifests can be inspected and validated; they do not launch
services through the plugin interface yet.

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
