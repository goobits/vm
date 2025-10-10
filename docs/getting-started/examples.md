# 📝 Common Examples

Real-world configuration examples for different project types and use cases.

## 🎯 Frontend Projects

### React Development
```yaml
# vm.yaml
os: ubuntu
provider: docker
project:
  name: react-app
  hostname: dev.react-app.local
ports:
  dev: 3000
  storybook: 6006
npm_packages:
  - "@storybook/cli"
  - prettier
  - eslint
```

### Vue.js with Vite
```yaml
# vm.yaml
os: ubuntu
provider: docker
project:
  name: vue-app
ports:
  dev: 5173
  preview: 4173
npm_packages:
  - "@vitejs/plugin-vue"
  - vite
aliases:
  dev: "npm run dev"
  build: "npm run build && npm run preview"
```

## 🔧 Backend Projects

### Django API
```yaml
# vm.yaml
os: ubuntu
provider: docker
project:
  name: django-api
  hostname: dev.django-api.local
ports:
  api: 8000
  postgresql: 5432
  redis: 6379
services:
  postgresql:
    enabled: true
    database: myapp_dev
    user: postgres
    password: postgres
  redis:
    enabled: true
pip_packages:
  - django-debug-toolbar
  - django-extensions
aliases:
  migrate: "python manage.py migrate"
  shell: "python manage.py shell_plus"
```

### Rails Application
```yaml
# vm.yaml
os: ubuntu
provider: docker
project:
  name: rails-app
  hostname: dev.rails-app.local
ports:
  web: 3000
  postgresql: 5432
  redis: 6379
environment:
  RAILS_ENV: development
  DATABASE_URL: postgresql://postgres:postgres@localhost:5432/rails_dev
aliases:
  console: "rails console"
  migrate: "rails db:migrate"
```

```yaml
# ~/.vm/config.yaml - Global services
services:
  postgresql:
    enabled: true
    database: rails_dev
  redis:
    enabled: true
```

## 🔗 Full-Stack Projects

### React + Node.js API
```yaml
# vm.yaml
os: ubuntu
provider: docker
project:
  name: fullstack-app
  hostname: dev.fullstack-app.local
ports:
  frontend: 3000
  backend: 3001
  postgresql: 5432
  redis: 6379
npm_packages:
  - nodemon
  - "@types/node"
  - concurrently
aliases:
  dev: "concurrently \"cd frontend && npm start\" \"cd backend && npm run dev\""
  test: "cd backend && npm test"
  migrate: "cd backend && npm run migrate"
```

```yaml
# ~/.vm/config.yaml - Global services
services:
  postgresql:
    enabled: true
    database: app_dev
  redis:
    enabled: true
```

### Vue + Django
```yaml
# vm.yaml
os: ubuntu
provider: docker
project:
  name: vue-django
  hostname: dev.vue-django.local
vm:
  memory: 6144  # More RAM for full-stack
ports:
  frontend: 8080
  api: 8000
  postgresql: 5432
npm_packages:
  - "@vue/cli"
pip_packages:
  - djangorestframework
  - django-cors-headers
aliases:
  dev-frontend: "cd frontend && npm run serve"
  dev-backend: "cd backend && python manage.py runserver"
  dev-all: "concurrently \"cd frontend && npm run serve\" \"cd backend && python manage.py runserver\""
```

```yaml
# ~/.vm/config.yaml - Global services
services:
  postgresql:
    enabled: true
    database: vuedjango_dev
```

## 🚀 Specialized Environments

### Mobile Development Backend
```yaml
# vm.yaml
os: ubuntu
provider: docker
project:
  name: mobile-backend
  hostname: dev.mobile-backend.local
vm:
  memory: 8192  # More RAM for mobile tooling
  port_binding: "0.0.0.0"  # Network accessible for devices
ports:
  api: 3000
  websocket: 3001
  postgresql: 5432
  redis: 6379
npm_packages:
  - "@react-native-community/cli"
  - socket.io
environment:
  NODE_ENV: development
  CORS_ORIGIN: "*"  # Allow mobile device access
```

```yaml
# ~/.vm/config.yaml - Global services
services:
  postgresql:
    enabled: true
  redis:
    enabled: true
  docker:
    enabled: true  # For containerized services
```

### Machine Learning / Data Science
```yaml
# vm.yaml
os: ubuntu
provider: docker
project:
  name: ml-project
  hostname: dev.ml-project.local
vm:
  memory: 12288  # 12GB for ML workloads
  cpus: 6
ports:
  jupyter: 8888
  tensorboard: 6006
  api: 5000
pip_packages:
  - jupyter
  - tensorflow
  - pytorch
  - pandas
  - scikit-learn
  - matplotlib
environment:
  JUPYTER_ENABLE_LAB: "yes"
aliases:
  notebook: "jupyter lab --ip=0.0.0.0 --port=8888 --no-browser --allow-root"
  tensorboard: "tensorboard --logdir=./logs --host=0.0.0.0 --port=6006"
```

### Multi-Language Project
```yaml
# vm.yaml
os: ubuntu
provider: docker
project:
  name: polyglot-project
  hostname: dev.polyglot-project.local
vm:
  memory: 8192
ports:
  rust_server: 8080
  python_api: 8000
  node_frontend: 3000
# Install all language runtimes
cargo_packages:
  - cargo-watch
  - serde_json
pip_packages:
  - fastapi
  - uvicorn
npm_packages:
  - vite
  - "@vitejs/plugin-react"
aliases:
  rust-dev: "cd rust-service && cargo watch -x run"
  python-dev: "cd python-api && uvicorn main:app --reload --host 0.0.0.0 --port 8000"
  frontend-dev: "cd frontend && npm run dev"
  dev-all: "concurrently \"rust-dev\" \"python-dev\" \"frontend-dev\""
```

## 🧪 Development Patterns

### Microservices Development
```yaml
# vm.yaml
os: ubuntu
provider: docker
project:
  name: microservices
  hostname: dev.microservices.local
vm:
  memory: 10240  # Large memory for multiple services
ports:
  gateway: 8080
  user_service: 8081
  order_service: 8082
  notification_service: 8083
  postgresql: 5432
  redis: 6379
aliases:
  start-all: "docker-compose up -d"
  logs-all: "docker-compose logs -f"
  stop-all: "docker-compose down"
```

```yaml
# ~/.vm/config.yaml - Global services
services:
  postgresql:
    enabled: true
  redis:
    enabled: true
  docker:
    enabled: true  # For service containers
```

### Database-Heavy Development
```yaml
# vm.yaml
os: ubuntu
provider: docker
project:
  name: data-heavy
  hostname: dev.data-heavy.local
  backup_pattern: "*backup*.sql.gz"  # Auto-restore backups
vm:
  memory: 8192
ports:
  api: 8000
  postgresql: 5432
  mongodb: 27017
  redis: 6379
  pgadmin: 5050
```

```yaml
# ~/.vm/config.yaml - Global services
services:
  postgresql:
    enabled: true
    database: primary_db
  mongodb:
    enabled: true
  redis:
    enabled: true
```

## 🐳 Docker Registry Caching

### Simple Docker Registry Setup
```yaml
# vm.yaml - Project configuration
os: ubuntu
provider: docker
project:
  name: docker-app
```

```yaml
# ~/.vm/config.yaml - Global Docker registry caching
services:
  docker_registry:
    enabled: true  # That's it! Auto-managed Docker cache
```

### Docker Registry with Custom Settings
```yaml
# vm.yaml - Project configuration
os: ubuntu
provider: docker
project:
  name: docker-heavy-app
```

```yaml
# ~/.vm/config.yaml - Advanced registry configuration
services:
  docker_registry:
    enabled: true
    max_cache_size_gb: 20        # 20GB cache for large images
    max_image_age_days: 90       # Keep images for 3 months
    cleanup_interval_hours: 4    # Less frequent cleanup
    enable_lru_eviction: true    # Auto-remove least used when full
    enable_auto_restart: true    # Restart on failures
    health_check_interval_minutes: 60  # Hourly health checks
```

### Microservices with Docker Caching
```yaml
# vm.yaml - Microservices benefit greatly from image caching
os: ubuntu
provider: docker
project:
  name: microservices-platform
vm:
  memory: 12288
ports:
  gateway: 8080
  services: 8081-8090
aliases:
  build-all: "docker-compose build --parallel"
  pull-all: "docker-compose pull"  # Uses local cache after first pull
```

```yaml
# ~/.vm/config.yaml - Global services
services:
  docker:
    enabled: true  # Docker-in-Docker for building
  docker_registry:
    enabled: true  # Cache all pulled images
    max_cache_size_gb: 15  # Large cache for many microservice images
```

### CI/CD Development Environment
```yaml
# vm.yaml - Fast CI/CD testing with cached images
os: ubuntu
provider: docker
project:
  name: cicd-testing
ports:
  jenkins: 8080
  nexus: 8081
environment:
  DOCKER_REGISTRY_MIRROR: "http://127.0.0.1:5000"
```

```yaml
# ~/.vm/config.yaml - Global services
services:
  docker:
    enabled: true
  docker_registry:
    enabled: true
    max_cache_size_gb: 10
    max_image_age_days: 7  # Short retention for CI images
    cleanup_interval_hours: 1  # Aggressive cleanup
```

## 🎨 Customization Patterns

### Port Strategy (Team Development)
```yaml
# Project 1: ports 3000-3009
os: ubuntu
provider: docker
project:
  name: team-project-1
ports:
  frontend: 3000
  backend: 3001
  database: 3002

# Project 2: ports 3010-3019
os: ubuntu
provider: docker
project:
  name: team-project-2
ports:
  frontend: 3010
  backend: 3011
  database: 3012

# Project 3: ports 3020-3029
os: ubuntu
provider: docker
project:
  name: team-project-3
ports:
  frontend: 3020
  backend: 3021
  database: 3022
```

### Environment-Specific Configs
```bash
# Different configs for different purposes
vm --config dev.yaml create      # Development
vm --config testing.yaml create   # Testing environment
vm --config staging.yaml create   # Staging mirror
```

## 💡 Tips & Tricks

### Resource Optimization
```yaml
# Lightweight for simple projects
os: ubuntu
provider: docker
project:
  name: lightweight-project
vm:
  memory: 2048
  cpus: 1

# Heavy for complex workloads
os: ubuntu
provider: docker
project:
  name: heavy-project
vm:
  memory: 12288
  cpus: 8
```

### Network Access
```yaml
# Local development (default)
os: ubuntu
provider: docker
project:
  name: local-dev-project
vm:
  port_binding: 127.0.0.1

# Team sharing / mobile testing
os: ubuntu
provider: docker
project:
  name: shared-dev-project
vm:
  port_binding: "0.0.0.0"
```

### Auto Defaults
```yaml
# Let the tool figure it out
os: ubuntu
provider: docker
project:
  name: auto-config-project
# Everything else auto-configured

# vs explicit control
os: ubuntu
provider: docker
project:
  name: explicit-config-project
vm:
  memory: 4096
  # ... many more options
```