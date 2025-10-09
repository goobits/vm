# Django Development Plugin

Python web framework environment with PostgreSQL and Redis support.

## What's Included

### Python Packages
- `django` - Web framework
- `djangorestframework` - REST API toolkit
- `django-cors-headers` - CORS handling
- `django-extensions` - Additional management commands
- `python-decouple` - Configuration management
- `psycopg2-binary` - PostgreSQL adapter
- `redis` - Redis client
- `celery` - Task queue
- `django-debug-toolbar` - Debug tools
- `pytest-django` - Testing framework
- `factory-boy` - Test fixtures

### NPM Packages
- `prettier` - Code formatter

### Environment Variables
- `DJANGO_SETTINGS_MODULE` - Specifies the settings module for Django.
- `DEBUG` - Enables debug mode in Django.
- `PYTHONDONTWRITEBYTECODE` - Prevents Python from writing .pyc files.

### Included Services
This preset includes the following services **enabled by default**:
- **PostgreSQL** - Relational database
- **Redis** - In-memory data store

To disable or customize services, see the [Configuration](#configuration) section below.

## Installation

This plugin is automatically installed with the VM tool. No additional installation required.

To verify availability:
```bash
vm config preset --list | grep django
```

## Usage

Apply this preset to your project:
```bash
vm config preset django
vm create
```

Or add to `vm.yaml`:
```yaml
preset: django
```

## Configuration

### Customizing Services
```yaml
preset: django
services:
  postgresql:
    enabled: false  # Disable service
    # Or customize:
    port: 5433
    database: myapp_dev
  redis:
    enabled: false
```

### Additional Packages
```yaml
preset: django
packages:
  pip:
    - new-python-package
  npm:
    - new-npm-package
```

## Common Use Cases

1. **Running Database Migrations**
   ```bash
   vm exec "python manage.py migrate"
   ```

2. **Starting the Development Server**
   ```bash
   vm exec "python manage.py runserver 0.0.0.0:8000"
   ```

## Troubleshooting

### Issue: PostgreSQL connection refused
**Solution**: Ensure the `postgresql` service is enabled in your `vm.yaml` and that the port is correctly mapped. Run `vm status` to check port mappings.

### Issue: Missing Python dependencies
**Solution**: Add the required packages to a `requirements.txt` file in your project or directly to your `vm.yaml` under `packages.pip`.

## Related Documentation

- [Configuration Guide](../../docs/user-guide/configuration.md)
- [Presets Overview](../../docs/user-guide/presets.md)
- [CLI Reference](../../docs/user-guide/cli-reference.md)

## License

MIT