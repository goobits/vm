# Installation

Install `vm` from a source checkout:

```bash
git clone https://github.com/goobits/vm.git
cd vm
./install.sh
```

Open a new terminal after installation so `vm` is on your `PATH`.

The bundled `vm.yaml` uses Docker. On macOS, install and start
[Docker Desktop](https://docs.docker.com/desktop/setup/install/mac-install/),
open a new terminal, then run `vm doctor`. On Linux, install Docker Engine.
Podman can be selected explicitly as an alternative provider. Provider
installation includes host services and operating-system setup.

The first environment using the bundled Vibe preset automatically builds its
reusable `@vibe-image` base. This initial build can take several minutes;
subsequent environments reuse it.

Verify:

```bash
vm --help
vm doctor
```

Start an environment:

```bash
vm init
vm create dev --provider docker --image ubuntu:24.04
vm start dev
```

macOS environments require Apple Silicon macOS and Tart:

```bash
vm create xcode --provider tart --image <macos-base-image>
vm start xcode
```

Advanced self-management:

```bash
vm system update
vm system update --version vX.Y.Z
vm system uninstall
vm system uninstall --keep-config
```

Shell completions are installed by the installer when supported by your shell.

Source installations atomically copy the finished executable into the stable
user binary directory (`~/.local/bin` on macOS and Linux). Reusable Cargo
artifacts live in the platform VM cache, never underneath the installed
executable, so pruning the build cache cannot break `vm`. Set
`CARGO_TARGET_DIR` to override the source-build cache location.
