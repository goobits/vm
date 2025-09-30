# Python Development Plugin

General Python development environment with testing, linting, and modern tooling.

## What's Included

### Python Packages
- `black` - Code formatter
- `flake8` - Linter
- `pytest` - Testing framework
- `ipython` - Enhanced REPL
- `pip-tools` - Dependency management
- `git-filter-repo` - Git history tools
- `pre-commit` - Git hooks framework

### NPM Packages
- `pyright` - Python LSP server

### Optional Services
- PostgreSQL (enable manually if needed)
- Redis (enable manually if needed)

### Environment
- `PYTHONDONTWRITEBYTECODE=1` - Prevent .pyc files

## Installation

```bash
vm plugin install plugins/python-dev
```

## Usage

```bash
vm config preset python
```

## License

MIT