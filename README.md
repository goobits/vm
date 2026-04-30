# VM

Humane virtual environments for development. Ask for the environment you want, give it a name in plain language, and let `vm` route to the right engine.

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

## Everyday Workflow

```bash
vm run linux as api
vm shell api
vm exec api -- cargo test
vm logs api --follow
vm restart api
vm stop api
vm remove api
```

`vm run` creates the environment if it does not exist and starts it if it is stopped.
`vm list` lists environments for the current project. Use `vm list --all` for the global inventory.

## Environment Kinds

```bash
vm run mac as xcode                    # Tart macOS VM
vm run mac                             # Tart macOS VM, addressed as `mac`
vm run linux as backend                # Docker Linux dev environment
vm run container as redis              # Docker container environment
vm run linux as secure --provider tart # Advanced routing override
vm run container as db --provider podman
```

Naming is intentionally natural language: use `as <name>`.

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

## State

```bash
vm save backend as stable
vm revert backend stable
vm package backend --output backend.tar.gz
```

`vm remove` removes active environment resources but keeps explicitly saved snapshots.

## Advanced Tools

```bash
vm config show
vm config set vm.memory 8192
vm tunnel add 8080:3000 backend
vm tunnel ls
vm doctor
vm system update
vm system registry status
vm system base validate vibe --provider docker
```

## Plugin-Backed Commands

Plugin-backed workflows stay flat and user-facing:

```bash
vm db ls
vm db backup app_db
vm fleet ls --provider docker
vm secret add OPENAI_API_KEY sk-...
vm plugin ls
vm plugin install ./plugins/db
```

The user experience is flat; the implementation can still come from plugins.

## Configuration

A project can use `vm.yaml` for persistent defaults. `vm run` creates a starter config when one is missing.

```yaml
version: '2.0'
provider: docker
project:
  name: backend
  workspace_path: /workspace
vm:
  box: ubuntu:24.04
  memory: 8192
  cpus: 4
```

Provider names are advanced routing details. Prefer `mac`, `linux`, and `container` in daily use.

## Development

```bash
cd rust
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
