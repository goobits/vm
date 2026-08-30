<h1 align="center">vm</h1>

<p align="center"><strong>Create named development environments with one CLI across supported providers.</strong></p>
<p align="center">Use Docker by default, Tart for macOS guests on Apple Silicon, or an explicitly selected compatible provider.</p>

<p align="center">
  <a href="#why-vm">Why vm</a> ·
  <a href="#quick-start">Quick start</a> ·
  <a href="#daily-workflow">Daily workflow</a> ·
  <a href="#state-and-provider-boundary">State and providers</a> ·
  <a href="#documentation">Documentation</a>
</p>

---

## Why vm

Development environments often require different provider commands, state
locations, network setup, and packaging steps. `vm` gives projects one named
environment model and keeps provider-specific behavior behind explicit
configuration.

## Quick start

Build and install from source, then verify local capabilities:

```bash
git clone https://github.com/goobits/vm.git
cd vm
./install.sh
vm --help
vm doctor
```

Start a Linux environment with the default provider:

```bash
vm run linux as dev
vm shell dev
```

## Daily workflow

| Command | Purpose |
| --- | --- |
| `vm run <kind> as <name>` | Create or start a named macOS, Linux, or container environment |
| `vm shell <name>` | Open an interactive shell |
| `vm exec <name> -- <command>` | Run a command in an environment |
| `vm restart <name>` | Restart an environment |
| `vm list` | List managed environments |
| `vm remove <name>` | Remove an environment |
| `vm doctor` | Check host and provider capabilities |

State workflows include save, revert, and package operations. Configuration,
tunnels, plugins, databases, secrets, and self-management have dedicated
command groups. Use `vm <command> --help` for exact syntax.

## State and provider boundary

Docker is the default Linux/container provider. Tart-based macOS environments
require Apple Silicon macOS and Tart. Podman support is optional and must be
selected where supported.

Save, revert, remove, package, database, secret, and system commands can change
durable environment state. Inspect the selected environment and provider before
running them. A configuration file does not install or license a missing guest
image, platform SDK, or provider.

## Documentation

- [Documentation index](docs/README.md)
- [Quick start](docs/getting-started/quick-start.md)
- [Examples](docs/getting-started/examples.md)
- [CLI reference](docs/user-guide/cli-reference.md)
- [Configuration](docs/user-guide/configuration.md)
- [Plugins](docs/user-guide/plugins.md)
- [Troubleshooting](docs/user-guide/troubleshooting.md)

The maintained installation guide still repeats the unsupported curl-pipe
pattern. Use the source checkout installation above until that guide and
installer are reconciled.

## License

[MIT](LICENSE).
