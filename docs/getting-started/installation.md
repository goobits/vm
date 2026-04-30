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
vm system update --version v5.0.1
vm system uninstall
vm system uninstall --keep-config
```

Shell completions are installed by the installer when supported by your shell.
