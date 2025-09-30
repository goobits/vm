# Node.js Development Plugin

Optimized environment for JavaScript/TypeScript projects with modern tooling.

## What's Included

### NPM Packages
- `prettier` - Code formatter
- `eslint` - Linter
- `typescript` - TypeScript compiler
- `ts-node` - TypeScript execution
- `nodemon` - Auto-reload dev server
- `npm-check-updates` - Dependency updater

### Python Tools
- `git-filter-repo` - Git history tools

### Optional Services
- Redis (enable manually if needed)
- PostgreSQL (enable manually if needed)

### Environment
- `NODE_ENV=development`

## Installation

```bash
vm plugin install plugins/nodejs-dev
```

## Usage

Create VM with this preset:

```bash
vm config preset nodejs
```

Or add to vm.yaml:

```yaml
preset: nodejs
```

## License

MIT