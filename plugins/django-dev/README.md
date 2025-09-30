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

### Optional Services
- PostgreSQL (enable manually if needed)
- Redis (enable manually if needed)

### Environment
- `DJANGO_SETTINGS_MODULE=settings.development`
- `DEBUG=True`
- `PYTHONDONTWRITEBYTECODE=1`

## Installation

```bash
vm plugin install plugins/django-dev
```

## Usage

```bash
vm config preset django
```

## License

MIT