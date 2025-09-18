# 🎨 Preset Format Specification

Complete reference for creating and understanding VM tool presets.

## 📋 Preset Structure

```yaml
# configs/presets/example.yaml
preset:
  name: "Example Framework"
  description: "Development environment for Example Framework"
  priority: 10                    # Optional: merge priority (default: 5)
  tags: ["frontend", "javascript"] # Optional: categorization

# Standard configuration sections
npm_packages:
  - example-cli
  - example-dev-tools

ports:
  dev: 3000
  build: 3001

services:
  postgresql:
    enabled: true
  redis:
    enabled: true

vm:
  memory: 4096
  cpus: 2

aliases:
  start: "example start"
  test: "example test"

environment:
  NODE_ENV: development
  EXAMPLE_MODE: dev

terminal:
  emoji: "🚀"
  theme: tokyo_night
```

## 🎯 Preset Metadata

### Required Fields
```yaml
preset:
  name: string        # Human-readable preset name
  description: string # Brief description of preset purpose
```

### Optional Metadata
```yaml
preset:
  priority: integer   # Merge priority (1-10, default: 5)
  tags: string[]      # Categories for filtering/search
  author: string      # Preset author
  version: string     # Preset version
  homepage: string    # Documentation URL
  dependencies: string[] # Required presets
  conflicts: string[] # Conflicting presets
```

**Priority System**:
- Higher numbers = higher priority
- Used when multiple presets modify same field
- User config always wins (priority: ∞)

## 🧩 Configuration Sections

### Package Management
```yaml
# Node.js packages
npm_packages:
  - package-name
  - "@scoped/package"
  - "package@version"

# Rust packages (triggers Rust installation)
cargo_packages:
  - cargo-watch
  - tokei

# Python packages (triggers Python installation)
pip_packages:
  - black
  - pytest
  - requests
```

### Service Configuration
```yaml
services:
  postgresql:
    enabled: true
    database: preset_db      # Optional: custom database name
    user: preset_user        # Optional: custom user
    password: preset_pass    # Optional: custom password
  redis:
    enabled: true
  mongodb:
    enabled: false
  docker:
    enabled: true           # Docker-in-Docker
  headless_browser:
    enabled: false          # Chrome/Chromium
```

### Port Assignments
```yaml
ports:
  dev: 3000              # Development server
  build: 3001            # Build server
  test: 3002             # Test server
  docs: 3003             # Documentation
  api: 8000              # API server
  database: 5432         # Database
  cache: 6379            # Cache service
```

**Port Naming Conventions**:
- Use descriptive names: `frontend`, `backend`, `api`
- Avoid generic names: `port1`, `port2`
- Follow framework conventions where possible

### VM Resource Recommendations
```yaml
vm:
  memory: 4096           # MB - framework requirements
  cpus: 2                # Cores - parallel build needs
  port_binding: 127.0.0.1 # or "0.0.0.0" for network access
```

**Resource Guidelines**:
- **Lightweight**: 1-2GB RAM (alpine, simple apps)
- **Standard**: 4GB RAM (most web frameworks)
- **Heavy**: 8GB+ RAM (ML, databases, microservices)

### Shell Customization
```yaml
aliases:
  dev: "npm run dev"
  start: "npm start"
  test: "npm test"
  build: "npm run build"
  lint: "npm run lint"

environment:
  NODE_ENV: development
  FRAMEWORK_MODE: dev
  DEBUG: framework:*

terminal:
  emoji: "⚛️"
  username: developer
  theme: one_dark
  show_git_branch: true
```

## 📁 Framework-Specific Examples

### React Preset
```yaml
# configs/presets/react.yaml
preset:
  name: "React"
  description: "React development environment"
  tags: ["frontend", "javascript", "spa"]

npm_packages:
  - "@vitejs/plugin-react"
  - "eslint-plugin-react"
  - "eslint-plugin-react-hooks"

ports:
  dev: 3000
  build: 3001

aliases:
  dev: "npm run dev"
  start: "npm start"
  build: "npm run build"
  test: "npm test"

environment:
  FAST_REFRESH: true
  GENERATE_SOURCEMAP: true

terminal:
  emoji: "⚛️"
  theme: one_dark
```

### Django Preset
```yaml
# configs/presets/django.yaml
preset:
  name: "Django"
  description: "Django web framework environment"
  tags: ["backend", "python", "web"]

services:
  postgresql:
    enabled: true
    database: django_dev
  redis:
    enabled: true

ports:
  dev: 8000
  postgresql: 5432
  redis: 6379

pip_packages:
  - django-debug-toolbar
  - django-extensions
  - ipython

aliases:
  manage: "python manage.py"
  shell: "python manage.py shell_plus"
  migrate: "python manage.py migrate"
  collectstatic: "python manage.py collectstatic --noinput"

environment:
  DJANGO_SETTINGS_MODULE: project.settings.development
  DEBUG: true

terminal:
  emoji: "🐍"
  theme: gruvbox_dark
```

### Full-Stack Preset
```yaml
# configs/presets/fullstack.yaml
preset:
  name: "Full-Stack"
  description: "Complete web application stack"
  tags: ["fullstack", "web", "database"]
  dependencies: ["react", "nodejs"]  # Inherits from other presets

services:
  postgresql:
    enabled: true
  redis:
    enabled: true

ports:
  frontend: 3000
  backend: 3001
  postgresql: 5432
  redis: 6379

npm_packages:
  - concurrently
  - nodemon

aliases:
  dev-frontend: "cd frontend && npm start"
  dev-backend: "cd backend && npm run dev"
  dev-all: "concurrently \"npm run dev-frontend\" \"npm run dev-backend\""

vm:
  memory: 6144  # More RAM for full stack

terminal:
  emoji: "🚀"
```

## 🔄 Preset Composition

### Inheritance
```yaml
# Child preset inheriting from parent
preset:
  name: "Next.js"
  description: "Next.js React framework"
  dependencies: ["react", "nodejs"]  # Inherits configurations

# Additional Next.js specific configuration
npm_packages:
  - "next"
  - "@next/bundle-analyzer"

ports:
  dev: 3000
  build: 3001

aliases:
  dev: "next dev"
  build: "next build"
  start: "next start"
```

### Composition Rules
1. **Dependencies loaded first**: Base presets applied before specific ones
2. **Priority ordering**: Higher priority presets override lower ones
3. **Array merging**: Package lists are combined (no duplicates)
4. **Object merging**: Service configs are merged recursively
5. **User override**: User config always takes precedence

### Conflict Resolution
```yaml
preset:
  name: "Conflicting Preset"
  conflicts: ["other-preset"]  # Cannot be used together

# Or handle conflicts programmatically
ports:
  dev: 3000  # Will override if higher priority
```

## 🎛️ Advanced Features

### Conditional Configuration
```yaml
# Platform-specific configuration
macos:
  vm:
    memory: 8192  # More RAM on macOS

linux:
  vm:
    memory: 4096

# Provider-specific configuration
docker:
  services:
    postgresql:
      enabled: true

vagrant:
  services:
    postgresql:
      enabled: false  # Use system PostgreSQL
```

### Dynamic Values
```yaml
environment:
  PROJECT_NAME: "${project.name}"           # Reference project config
  DATABASE_URL: "postgresql://localhost:${ports.postgresql}/${services.postgresql.database}"
```

### Template Variables
```yaml
aliases:
  deploy: "deploy.sh ${project.name} ${project.hostname}"

environment:
  API_ENDPOINT: "http://${project.hostname}:${ports.api}"
```

## 🧪 Detection Patterns

### File-Based Detection
```yaml
# Detection configuration (in project-detector.sh)
preset_detection:
  patterns:
    - file: "package.json"
      content: '"react":'
      preset: "react"
    - file: "manage.py"
      preset: "django"
    - file: "Gemfile"
      content: 'gem ["\']rails'
      preset: "rails"
```

### Multi-Technology Detection
```yaml
# Project with multiple technologies detected
detected_presets: ["react", "nodejs", "docker"]

# Preset application order:
# 1. docker (priority: 3)
# 2. nodejs (priority: 5)
# 3. react (priority: 5)
# 4. user config (priority: ∞)
```

## 📝 Preset Validation

### Schema Validation
```bash
# Validate preset format
vm preset validate configs/presets/my-preset.yaml

# Test preset application
vm preset test my-preset
```

### Required Elements
- Valid YAML syntax
- Required `preset.name` and `preset.description`
- Valid port numbers (1024-65535)
- Valid service configurations
- Valid package names

### Best Practices
1. **Clear naming**: Use descriptive, searchable names
2. **Focused scope**: One framework/technology per preset
3. **Minimal defaults**: Only include essential configuration
4. **Documentation**: Include helpful descriptions and examples
5. **Testing**: Test preset application and conflicts

## 🚀 Custom Preset Creation

### Development Workflow
```bash
# 1. Create preset file
vi configs/presets/my-framework.yaml

# 2. Add detection logic
vi shared/project-detector.sh
# Add detect_my_framework() function

# 3. Test detection
echo "my-framework-project" > /tmp/test
cd /tmp/test
touch my-framework.config.js
vm preset list --detected

# 4. Test preset application
vm config preset my-framework  # Apply preset to existing config
# Or let auto-detection use it
vm create

# 5. Add tests
vi test/unit/preset-detection.test.sh
# Add test_detect_my_framework()
```

### Distribution
```bash
# Include preset in project
configs/presets/my-preset.yaml

# External preset (future feature)
~/.vm/presets/my-preset.yaml

# Shared preset repository (planned)
vm preset install author/my-preset
```

## 🔍 Debugging Presets

### Preset Application Debug
```bash
# See which presets are detected/applied
LOG_LEVEL=DEBUG vm create

# Show effective configuration after preset application
vm preset show --effective

# Validate preset merge results
vm validate --verbose
```

### Common Issues
1. **Port conflicts**: Multiple presets using same ports
2. **Package conflicts**: Incompatible package versions
3. **Resource conflicts**: Insufficient memory/CPU allocation
4. **Service conflicts**: Multiple database services enabled
5. **Detection failures**: Incorrect file pattern matching