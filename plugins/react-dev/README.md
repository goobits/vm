# React Development Plugin

Modern frontend development environment with React, Vite, and testing tools.

## What's Included

### NPM Packages
- `create-react-app` - Set up a modern React web app by running one command.
- `react-router-dom` - DOM bindings for React Router.
- `vite` - Next Generation Frontend Tooling.
- `@vitejs/plugin-react` - The official Vite plugin for React.
- `jest` - A delightful JavaScript Testing Framework.
- `react-testing-library` - Simple and complete React DOM testing utilities.

### Python Packages
- `git-filter-repo` - For advanced Git history rewriting.

### Environment Variables
- `NODE_ENV` - Set to `development` by default.
- `REACT_APP_ENV` - Custom environment variable for Create React App, set to `development`.
- `FAST_REFRESH` - Enables Vite's fast refresh feature.

### Included Services
This preset can be configured to use services, but none are enabled by default.
- **Redis** - In-memory data store.
- **PostgreSQL** - Relational database.

To enable a service, see the [Configuration](#configuration) section below.

## Installation

This plugin is automatically installed with the VM tool. No additional installation required.

To verify availability:
```bash
vm config preset --list | grep react
```

## Usage

Apply this preset to your project:
```bash
vm config preset react
vm create
```

Or add to `vm.yaml`:
```yaml
preset: react
```

## Configuration

### Customizing Services
```yaml
preset: react
services:
  postgresql:
    enabled: true
    database: my_react_app_db
```

### Additional Packages
```yaml
preset: react
packages:
  npm:
    - axios
    - moment
```

## Common Use Cases

1. **Starting the Vite Development Server**
   ```bash
   vm exec "npm run dev"
   ```

2. **Running Tests with Jest**
   ```bash
   vm exec "npm test"
   ```

## Troubleshooting

### Issue: Port conflict on `localhost:5173` (Vite) or `localhost:3000` (CRA)
**Solution**: The default port is in use. You can either stop the other process or configure a different port in your project's configuration file (e.g., `vite.config.js`).

### Issue: Missing npm packages after `git pull`
**Solution**: Your dependencies are out of sync. Run `vm exec "npm install"` to install any new packages.

## Related Documentation

- [Configuration Guide](../../docs/user-guide/configuration.md)
- [Presets Overview](../../docs/user-guide/presets.md)
- [CLI Reference](../../docs/user-guide/cli-reference.md)

## License

MIT