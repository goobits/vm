# Python Development Plugin

General Python development environment with testing, linting, and modern tooling.

## What's Included

### Python Packages
- `black` - The uncompromising Python code formatter.
- `flake8` - A tool to check your Python code against some of the style conventions in PEP 8.
- `pytest` - A framework that makes it easy to write small, readable tests.
- `ipython` - A powerful interactive Python shell.
- `pip-tools` - A set of tools to keep your pinned Python dependencies fresh.
- `pre-commit` - A framework for managing and maintaining multi-language pre-commit hooks.

### NPM Packages
- `pyright` - Static type checker for Python.

### Environment Variables
- `PYTHONDONTWRITEBYTECODE` - If set to a non-empty string, Python will not try to write `.pyc` files.

### Included Services
This preset includes the following services **enabled by default**:
- **PostgreSQL** - Relational database for your application.
- **Redis** - In-memory data store for caching or message brokering.

To disable or customize services, see the [Configuration](#configuration) section below.

## Installation

This plugin is automatically installed with the VM tool. No additional installation required.

To verify availability:
```bash
vm config preset --list | grep python
```

## Usage

Apply this preset to your project:
```bash
vm config preset python
vm create
```

Or add to `vm.yaml`:
```yaml
preset: python
```

## Configuration

### Customizing Services
```yaml
preset: python
services:
  postgresql:
    enabled: false
  redis:
    port: 6380
```

### Additional Packages
```yaml
preset: python
packages:
  pip:
    - requests
    - numpy
  npm:
    - some-js-tool
```

## Common Use Cases

1. **Running Pytest**
   ```bash
   vm exec "pytest"
   ```

2. **Formatting code with Black**
   ```bash
   vm exec "black ."
   ```

## Troubleshooting

### Issue: `ModuleNotFoundError: No module named '...'`
**Solution**: Ensure your dependencies are listed in a `requirements.txt` file at the root of your project, or add them to your `vm.yaml` under `packages.pip`. Run `vm exec "pip install -r requirements.txt"` if needed.

### Issue: Linting errors from `flake8`
**Solution**: Run `vm exec "flake8"` to see detailed error reports. You can configure `flake8` by adding a `.flake8` file to your project root.

## Related Documentation

- [Configuration Guide](../../docs/user-guide/configuration.md)
- [Presets Overview](../../docs/user-guide/presets.md)
- [CLI Reference](../../docs/user-guide/cli-reference.md)

## License

MIT