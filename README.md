# VM - Humane Virtual Environments

Ask for the environment you want, give it a name in plain language, and let `vm` route to the right engine.

```bash
vm run linux as backend
vm shell backend
vm exec backend -- npm test
vm restart backend
vm remove backend
```

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/goobits/vm/main/install.sh | bash
```

Docker is the default engine for Linux and container environments. Tart powers macOS environments on Apple Silicon macOS. Podman is available as an advanced provider override.

## Mental Model

You choose the kind of environment:

| Kind | Command | Default engine |
| --- | --- | --- |
| macOS VM | `vm run mac as xcode` | Tart |
| Linux dev env | `vm run linux as backend` | Docker |
| Container | `vm run container as redis` | Docker |

Provider names are escape hatches. Day to day, think in `mac`, `linux`, and `container`.

## Everyday Workflow

```bash
vm run linux as api
vm list
vm shell api
vm exec api -- cargo test
vm logs api --follow
vm copy ./config.json api:/workspace/config.json
vm restart api
vm stop api
vm remove api
```

`vm run` creates the environment if it does not exist and starts it if it is stopped.
`vm list` lists environments for the current project. Use `vm list --all` for the global inventory.

## Naming

```bash
vm run linux as backend
vm run mac as xcode
vm run container as redis
```

Naming is intentionally natural language: use `as <name>`.

If you skip `as <name>`, the kind becomes the name:

```bash
vm run mac
vm shell mac
```

## State

```bash
vm save backend as stable
vm revert backend stable
vm package backend --output backend.tar.gz
```

`vm remove` removes active environment resources but keeps explicitly saved snapshots.

## When You Need More

The daily surface stays small. Specialized workflows are still close by when you need them.

```bash
vm config show                         # inspect project defaults
vm config set vm.memory 8192           # tune resources
vm tunnel add 8080:3000 backend        # expose a port
vm doctor                              # diagnose engine issues
vm system update                       # update vm itself
```

For the complete command surface, see [docs/user-guide/cli-reference.md](docs/user-guide/cli-reference.md).

## Plugins

```bash
vm db backup app_db
vm secret add OPENAI_API_KEY sk-...
vm plugin install ./plugins/vibe-dev
```

Plugin-backed workflows stay flat and user-facing. You use `vm db`, `vm fleet`, or `vm secret`; the implementation can still come from plugins.

## Configuration

You can ignore configuration until the defaults are not enough. A project can use `vm.yaml` for durable choices like memory, CPU, workspace path, and default image. `vm run` creates a starter config when one is missing.

```yaml
version: '2.0'
provider: docker
project:
  name: backend
  workspace_path: /workspace
vm:
  image: ubuntu:24.04
  memory: 8192
  cpus: 4
```

## Advanced Capability: Docker Inside macOS VMs

`vm` can boot a real macOS Tart environment and run Docker workloads inside it.

```text
+----------------------+
| Apple Silicon Mac    |  M3/M4 + macOS 15+
+----------+-----------+
           | tart run --nested
+----------v-----------+
| macOS Dev VM         |  Xcode, Homebrew, /workspace
+----------+-----------+
           | Colima
+----------v-----------+
| Docker Workloads     |  build, compose, test
+----------------------+
```

That gives advanced workflows one interface for macOS tooling, Linux containers, and repeatable project environments. It requires an M3/M4 Mac host, macOS 15+ on the host, and a macOS 15+ guest.

## Development

```bash
cd rust
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
