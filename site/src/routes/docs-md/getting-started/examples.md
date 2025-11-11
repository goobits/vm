# Common Examples

Real-world configurations for different project types. All examples assume `os: ubuntu` and `provider: docker` unless noted.

**Resource Format Options**: `memory: 4096` (MB), `"4gb"`, `"50%"`, `"unlimited"` - same for cpus, swap, disk_size.
**See Full Reference**: [Configuration Guide](../user-guide/configuration.md) for all available options

## Frontend Projects

### React / Vue / Angular
```yaml
# vm.yaml
project:
  name: react-app
ports:
  dev: 3000       # React default, or 5173 for Vite, 4200 for Angular
  storybook: 6006
npm_packages:
  - "@storybook/cli"
  - prettier
  - eslint
aliases:
  dev: "npm start"  # or "npm run dev" for Vite
```

*See also*: [Presets Guide](../user-guide/presets.md) for auto-detected framework configs

## Backend Projects

### Python (Django / Flask) or Ruby (Rails)
```yaml
# vm.yaml
project:
  name: django-api  # or rails-app
ports:
  api: 8000         # Django, or 3000 for Rails
services:
  postgresql:
    enabled: true
    database: myapp_dev
  redis:
    enabled: true
pip_packages:       # or gem_packages for Rails
  - django-debug-toolbar
  - django-extensions
environment:
  RAILS_ENV: development  # Rails only
aliases:
  migrate: "python manage.py migrate"  # or "rails db:migrate"
  shell: "python manage.py shell_plus"   # or "rails console"
```

## Full-Stack Projects

```yaml
# vm.yaml - Any frontend + backend combo
project:
  name: fullstack-app
vm:
  memory: 6144  # More RAM for running both client and server
ports:
  frontend: 3000   # or 8080 for Vue, 4200 for Angular
  backend: 3001    # or 8000 for Django/Flask, 3000 for Rails
npm_packages:      # Frontend tools
  - concurrently
pip_packages:      # If using Python backend
  - djangorestframework
  - django-cors-headers
aliases:
  dev: "concurrently \"cd frontend && npm start\" \"cd backend && npm run dev\""
  migrate: "cd backend && npm run migrate"
```

```yaml
# ~/.vm/config.yaml - Shared database
services:
  postgresql:
    enabled: true
    database: app_dev
```

## Specialized Environments

### Machine Learning / Data Science
```yaml
# vm.yaml
project:
  name: ml-project
vm:
  memory: "12gb"
  cpus: 6
ports:
  jupyter: 8888
  tensorboard: 6006
pip_packages:
  - jupyter
  - tensorflow  # or pytorch
  - pandas
  - scikit-learn
aliases:
  notebook: "jupyter lab --ip=0.0.0.0 --port=8888 --no-browser"
  tensorboard: "tensorboard --logdir=./logs --host=0.0.0.0"
```

*Tip*: Use [base images](../../examples/base-images/python-ml.dockerfile) to pre-install ML libraries (saves 8+ minutes per VM)

## Advanced Patterns

### Custom Base Images

Speed up VM creation by pre-installing heavy dependencies (like Playwright, Chromium) in reusable Docker base images. See [examples/base-images/](../../examples/base-images/) for ready-to-use Dockerfiles and detailed guides.

**Quick example:**
```bash
# Build a base image with Playwright pre-installed
docker build -f examples/base-images/playwright-chromium.dockerfile -t my-base:latest .

# Use in your project
vm:
  box: my-base:latest  # Instead of ubuntu:24.04
```

VM creation is significantly faster (seconds instead of the usual 5+ minutes to install Playwright/Chromium).

### Multiple Databases
```yaml
# vm.yaml
project:
  name: data-app
  backup_pattern: "*backup*.sql.gz"  # Auto-restore on create
vm:
  memory: 8192
ports:
  api: 8000
```

```yaml
# ~/.vm/config.yaml - Multiple database services
services:
  postgresql:
    enabled: true
  mongodb:
    enabled: true
  redis:
    enabled: true
```

### Docker Image Caching

Speeds up `docker pull` by 10-100x after first cache. Enable globally for all VMs:

```yaml
# ~/.vm/config.yaml
services:
  docker_registry:
    enabled: true  # Auto-managed cache, zero maintenance
    max_cache_size_gb: 10           # Optional: customize settings
    max_image_age_days: 30          # Optional: retention policy
```

*See also*: [Shared Services](../user-guide/shared-services.md) for Docker registry configuration

## Configuration Tips

### Resource Allocation
- **Lightweight**: `memory: "2gb", cpus: "50%"` for simple projects
- **Heavy**: `memory: "12gb", cpus: "75%"` for ML, full-stack, or microservices
- **Percentage-based**: Adapts to host machine capabilities

### Network Access
- **Local only** (default): `port_binding: 127.0.0.1` - secure, single-user
- **Team sharing**: `port_binding: "0.0.0.0"` - accessible from other machines/devices

### Port Organization
Organize team projects with port ranges: project-1 uses 3000-3009, project-2 uses 3010-3019, etc.

### Multiple Configurations
```bash
vm --config dev.yaml create     # Development environment
vm --config testing.yaml create # Testing environment
```

### Auto-Detection
Minimal config works - VM auto-detects frameworks from your project files:
```yaml
project:
  name: my-app
# Detects Node.js, Python, Ruby, etc. and configures automatically
```