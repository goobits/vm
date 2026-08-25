# Installation

Install `vm`:

```bash
curl -fsSL https://raw.githubusercontent.com/goobits/vm/main/install.sh | bash
```

Verify:

```bash
vm --help
vm doctor
```

Start an environment:

```bash
vm run linux as dev
```

macOS environments require Apple Silicon macOS and Tart:

```bash
vm run mac as xcode
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
