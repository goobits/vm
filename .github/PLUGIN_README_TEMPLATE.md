# {Plugin Name} Development Plugin

{Brief 1-2 sentence description of the plugin purpose and target framework/language}

## What's Included

### System Packages
- `package-name` - Description of what it does

### NPM Packages (if applicable)
- `package-name` - Description
- `package-name` - Description

### Python Packages (if applicable)
- `package-name` - Description

### Ruby Gems (if applicable)
- `gem-name` - Description

### Environment Variables
- `VAR_NAME` - Description and purpose

### Included Services

This preset includes the following services **enabled by default**:
- **ServiceName** - Purpose

To disable or customize services, see the [Configuration](#configuration) section below.

## Installation

This plugin is automatically installed with the VM tool. No additional installation required.

To verify availability:

```bash
vm config preset --list | grep {preset-name}
```

## Usage

Apply this preset to your project:

```bash
vm config preset {preset-name}
vm create
```

Or add to `vm.yaml`:

```yaml
preset: {preset-name}
```

## Configuration

### Customizing Services

```yaml
preset: {preset-name}
services:
  servicename:
    enabled: false  # Disable service
    # Or customize:
    port: 5433
    database: custom_db
```

### Additional Packages

```yaml
preset: {preset-name}
packages:
  npm:
    - custom-package
  pip:
    - custom-python-package
```

## Common Use Cases

1. **{Use Case 1}**
   ```bash
   # Example commands
   ```

2. **{Use Case 2}**
   ```bash
   # Example commands
   ```

## Troubleshooting

### Issue: {Common Problem}
**Solution**: {How to fix}

### Issue: {Common Problem}
**Solution**: {How to fix}

## Related Documentation

- [Configuration Guide](../../docs/user-guide/configuration.md)
- [Presets Overview](../../docs/user-guide/presets.md)
- [CLI Reference](../../docs/user-guide/cli-reference.md)

## License

MIT