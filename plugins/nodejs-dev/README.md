# Node.js Development Plugin

Optimized environment for JavaScript/TypeScript projects with modern tooling.

## What's Included

### NPM Packages
- `prettier` - An opinionated code formatter.
- `eslint` - Pluggable and configurable linter tool for identifying and reporting on patterns in JavaScript.
- `typescript` - A typed superset of JavaScript that compiles to plain JavaScript.
- `ts-node` - TypeScript execution and REPL for Node.js.
- `nodemon` - A tool that helps develop Node.js based applications by automatically restarting the node application when file changes in the directory are detected.
- `npm-check-updates` - A command-line tool that allows you to upgrade your `package.json` dependencies to the latest versions.

### Python Packages
- `git-filter-repo` - For advanced Git history rewriting.

### Environment Variables
- `NODE_ENV` - Set to `development` by default to enable development-specific features in many libraries.

### Included Services
This preset includes the following services **enabled by default**:
- **Redis** - In-memory data store, used as a cache or message broker.
- **PostgreSQL** - Powerful, open source object-relational database system.

To disable or customize services, see the [Configuration](#configuration) section below.

## Installation

This plugin is automatically installed with the VM tool. No additional installation required.

To verify availability:
```bash
vm config preset --list | grep nodejs
```

## Usage

Apply this preset to your project:
```bash
vm config preset nodejs
vm create
```

Or add to `vm.yaml`:
```yaml
preset: nodejs
```

## Configuration

### Customizing Services
```yaml
preset: nodejs
services:
  redis:
    enabled: false
  postgresql:
    port: 5433
    database: my_node_app
```

### Additional Packages
```yaml
preset: nodejs
packages:
  npm:
    - express
  pip:
    - boto3
```

## Common Use Cases

1. **Running a Node.js script**
   ```bash
   vm exec "node my_script.js"
   ```

2. **Running a TypeScript project with nodemon**
   ```bash
   vm exec "nodemon --exec ts-node src/index.ts"
   ```

## Troubleshooting

### Issue: `Error: Cannot find module '...'`
**Solution**: This typically means npm packages are not installed. Run `vm exec "npm install"` inside your project directory within the VM.

### Issue: Service connection issues
**Solution**: Verify that the services (`postgresql`, `redis`) are enabled in your `vm.yaml` and check their status and port mappings with `vm status`.

## Related Documentation

- [Configuration Guide](../../docs/user-guide/configuration.md)
- [Presets Overview](../../docs/user-guide/presets.md)
- [CLI Reference](../../docs/user-guide/cli-reference.md)

## License

MIT