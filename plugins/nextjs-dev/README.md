# Next.js Development Plugin

Full-stack React framework environment with SSR, API routes, and modern tooling.

## What's Included

### NPM Packages
- `next` - The React framework for production.
- `@next/eslint-plugin-next` - ESLint plugin for Next.js specific rules.
- `sharp` - High-performance image processing.
- `typescript` - A typed superset of JavaScript.
- `eslint-config-next` - ESLint configuration for Next.js.
- `react-testing-library` - Simple and complete testing utilities for React.
- `jest` - A delightful JavaScript Testing Framework.

### Python Packages
- `git-filter-repo` - For advanced Git history rewriting.

### Environment Variables
- `NODE_ENV` - Set to `development` by default.
- `NEXT_PUBLIC_ENV` - Custom environment variable for frontend, set to `development`.
- `NEXT_TELEMETRY_DISABLED` - Disables Next.js telemetry.

### Included Services
This preset includes the following services **enabled by default**:
- **Redis** - In-memory data store for caching and sessions.
- **PostgreSQL** - Relational database for application data.

To disable or customize services, see the [Configuration](#configuration) section below.

## Installation

This plugin is automatically installed with the VM tool. No additional installation required.

To verify availability:
```bash
vm config preset --list | grep next
```

## Usage

Apply this preset to your project:
```bash
vm config preset next
vm create
```

Or add to `vm.yaml`:
```yaml
preset: next
```

## Configuration

### Customizing Services
```yaml
preset: next
services:
  redis:
    enabled: false
  postgresql:
    enabled: false
    port: 5433
    database: my_next_app
```

### Additional Packages
```yaml
preset: next
packages:
  npm:
    - zod
  pip:
    - requests
```

## Common Use Cases

1. **Starting the Development Server**
   ```bash
   vm exec "npm run dev"
   ```

2. **Running Tests**
   ```bash
   vm exec "npm test"
   ```

## Troubleshooting

### Issue: `Error: listen EADDRINUSE: address already in use :::3000`
**Solution**: The default port (3000) is already in use on your host. Map it to a different host port in your `vm.yaml` under the `ports` section.

### Issue: Image optimization errors with `sharp`
**Solution**: Ensure that the `sharp` package installed correctly. You may need to run `vm apply` to reinstall dependencies if you encounter native compilation issues.

## Related Documentation

- [Configuration Guide](../../docs/user-guide/configuration.md)
- [Presets Overview](../../docs/user-guide/presets.md)
- [CLI Reference](../../docs/user-guide/cli-reference.md)

## License

MIT