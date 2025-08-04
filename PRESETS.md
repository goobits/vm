# VM Tool Preset System Documentation

## Overview

The VM Tool includes an intelligent preset system that automatically configures virtual machines based on their intended use case. This system analyzes your configuration files and applies appropriate presets to ensure your VMs have all the necessary tools, ports, and resources for your development needs.

## Purpose and Benefits

### Why Use Presets?

1. **Simplified Configuration**: No need to manually specify every tool and port for common development scenarios
2. **Best Practices**: Presets include recommended resource allocations and tool combinations
3. **Consistency**: Ensures all team members have properly configured development environments
4. **Time Saving**: Reduces setup time from hours to minutes
5. **Flexibility**: Can be disabled or overridden when needed

### Key Features

- **Automatic Detection**: Analyzes your configuration to determine the appropriate preset
- **Forced Presets**: Explicitly specify which preset to use
- **Opt-out Option**: Disable presets entirely for full manual control
- **Additive Configuration**: Presets enhance rather than replace your configuration

## Available Presets

### Web Development Preset (`web`)

Optimized for frontend and full-stack web development.

**Included Tools:**
- `nginx` - Web server
- `nodejs` - JavaScript runtime
- `npm` - Node package manager
- `yarn` - Alternative package manager
- `git` - Version control
- `curl` - HTTP client
- `wget` - File downloader

**Ports Configured:**
- 80, 443 - HTTP/HTTPS
- 3000, 3001 - Common Node.js ports
- 4200 - Angular development server
- 5173 - Vite development server
- 8080, 8443 - Alternative HTTP/HTTPS

**Resources:**
- Memory: 4GB minimum
- CPUs: 2 (default)
- Disk: 20GB (default)

### Mobile Development Preset (`mobile`)

Configured for Android, iOS, and cross-platform mobile development.

**Included Tools:**
- `android-sdk` - Android development tools
- `flutter` - Cross-platform framework
- `dart` - Dart language runtime
- `react-native` - React Native framework
- `nodejs` - JavaScript runtime
- `npm` - Package manager
- `git` - Version control
- `adb` - Android Debug Bridge

**Ports Configured:**
- 8081 - React Native Metro bundler
- 8100 - Ionic development server
- 19000, 19001, 19002 - Expo development
- 5037 - ADB server

**Resources:**
- Memory: 8GB minimum
- CPUs: 4 minimum
- Disk: 20GB (default)

### Backend Development Preset (`backend`)

Ideal for API development, microservices, and server applications.

**Included Tools:**
- `docker` - Container runtime
- `docker-compose` - Multi-container orchestration
- `postgresql` - PostgreSQL database
- `mysql` - MySQL database
- `redis` - In-memory data store
- `mongodb` - NoSQL database
- `nginx` - Reverse proxy
- `git` - Version control

**Ports Configured:**
- 80, 443 - HTTP/HTTPS
- 3306 - MySQL
- 5432 - PostgreSQL
- 6379 - Redis
- 27017 - MongoDB
- 8000, 8080 - Application servers

**Resources:**
- Memory: 4GB minimum
- CPUs: 2 (default)
- Disk: 20GB (default)

### Data Science Preset (`data`)

Configured for data analysis, machine learning, and scientific computing.

**Included Tools:**
- `python3` - Python runtime
- `python3-pip` - Python package manager
- `jupyter` - Jupyter notebooks
- `numpy` - Numerical computing
- `pandas` - Data analysis
- `matplotlib` - Plotting library
- `scikit-learn` - Machine learning
- `tensorflow` - Deep learning
- `pytorch` - Deep learning
- `postgresql` - Relational database
- `mongodb` - Document database
- `elasticsearch` - Search and analytics
- `kibana` - Data visualization

**Ports Configured:**
- 8888, 8889 - Jupyter notebooks
- 5432 - PostgreSQL
- 27017 - MongoDB
- 9200 - Elasticsearch
- 5601 - Kibana

**Resources:**
- Memory: 8GB minimum
- CPUs: 4 minimum
- Disk: 50GB minimum

## Usage Patterns

### Automatic Detection (Default)

The preset system automatically detects the appropriate preset based on your configuration:

```bash
# config/web-app.yaml contains nginx or React references
./vm.sh -c config/web-app.yaml create my-web-vm
# Automatically applies web preset
```

### Disabling Presets

Use the `--no-preset` flag to disable all preset enhancements:

```bash
./vm.sh -c config/minimal.yaml --no-preset create my-minimal-vm
# Only uses configuration from minimal.yaml
```

### Forcing a Specific Preset

Use the `--preset` flag to force a specific preset regardless of configuration content:

```bash
./vm.sh -c config/app.yaml --preset backend create my-api-vm
# Forces backend preset even if config suggests otherwise
```

### Creating Partial Configurations

Leverage presets to create minimal configuration files:

```yaml
# config/my-web-app.yaml
VM_NAME="my-web-app"
TOOLS="react postgresql"  # Preset will add nginx, nodejs, npm, etc.
PORTS="5000"              # Preset will add 80, 443, 3000, etc.
```

## Examples and Use Cases

### Example 1: Quick Web Development VM

Create a simple configuration file:
```yaml
# config/react-app.yaml
VM_NAME="react-dev"
TOOLS="react"
```

Run:
```bash
./vm.sh -c config/react-app.yaml create react-dev
```

Result: VM with React, Node.js, npm, yarn, nginx, and all standard web development ports.

### Example 2: Custom Backend API VM

Create a configuration with specific requirements:
```yaml
# config/api-server.yaml
VM_NAME="api-prod"
VM_MEMORY="8192"
TOOLS="python3 fastapi"
PORTS="8000"
```

Run:
```bash
./vm.sh -c config/api-server.yaml --preset backend create api-prod
```

Result: VM with 8GB RAM, Python, FastAPI, plus Docker, PostgreSQL, Redis, and other backend tools.

### Example 3: Minimal VM Without Presets

Create a bare-bones configuration:
```yaml
# config/minimal.yaml
VM_NAME="test-vm"
VM_MEMORY="1024"
VM_CPUS="1"
TOOLS="vim curl"
```

Run:
```bash
./vm.sh -c config/minimal.yaml --no-preset create test-vm
```

Result: VM with exactly 1GB RAM, 1 CPU, and only vim and curl installed.

### Example 4: Data Science Workstation

Create a configuration for ML development:
```yaml
# config/ml-workstation.yaml
VM_NAME="ml-dev"
TOOLS="jupyter tensorflow"
MOUNTS="./datasets:/data"
```

Run:
```bash
./vm.sh -c config/ml-workstation.yaml create ml-dev
```

Result: VM with 8GB+ RAM, 4+ CPUs, 50GB+ disk, full data science stack, and mounted datasets.

## Best Practices

1. **Start with Presets**: Let the preset system handle common configurations
2. **Override When Needed**: Specify custom values in your config file to override preset defaults
3. **Use Partial Configs**: Define only what's unique to your project
4. **Document Exceptions**: If disabling presets, document why in your configuration
5. **Test Preset Detection**: Use `--dry-run` (if available) to preview the final configuration

## Troubleshooting

### Preset Not Detected
- Ensure your configuration includes recognizable tool names
- Use `--preset` to force the desired preset

### Wrong Preset Applied
- Check for conflicting tool names in your configuration
- Use `--preset` to override detection
- Use `--no-preset` for full manual control

### Resource Conflicts
- Preset minimum resources only apply if your config specifies less
- Your configuration values always take precedence when higher

## Advanced Configuration

### Combining Presets with Custom Tools

```yaml
# config/custom-stack.yaml
VM_NAME="custom-dev"
TOOLS="rust cargo"  # Your tools
# Preset will add web development tools automatically
```

### Preset-Aware Team Configurations

Create a base configuration that works well with presets:
```yaml
# config/team-base.yaml
VM_MEMORY="4096"
VM_CPUS="2"
ENVIRONMENT="TEAM=myteam ENV=development"
# Let presets handle tools and ports
```

Team members can then use:
```bash
./vm.sh -c config/team-base.yaml --preset web create john-web-vm
./vm.sh -c config/team-base.yaml --preset backend create jane-api-vm
```

## Customizing Presets

### Understanding Configuration Precedence

The VM tool uses a sophisticated layered configuration system that allows you to customize any aspect of your development environment while still benefiting from smart preset defaults. Understanding the merge order is crucial to effective customization:

```
Schema Defaults → Base Preset → Detected Preset → User vm.yaml
(lowest priority)                               (highest priority)
```

**Configuration Merge Order (detailed):**

1. **Schema Defaults** - Basic default values from `vm.schema.yaml` (memory: 2GB, cpus: 2, etc.)
2. **Base Preset** - Generic development tools applied to all configurations (`configs/presets/base.yaml`)
3. **Detected Preset** - Framework-specific preset based on your project type (`configs/presets/react.yaml`, `configs/presets/django.yaml`, etc.)
4. **User Configuration** - Your `vm.yaml` file settings (highest priority - always wins)

**Key Precedence Rules:**
- **Objects are merged deeply** - You can override specific nested properties without losing others
- **Arrays are replaced completely** - If you specify ports, you replace the entire ports array
- **User values always win** - Your `vm.yaml` settings override any preset values
- **Additive by design** - Presets add to your configuration, they don't restrict it

### Common Customization Scenarios

#### 1. Changing Service Settings

Override specific service configurations while keeping other preset benefits:

```yaml
# vm.yaml - Override PostgreSQL version from Django preset
project:
  name: "my-django-app"

services:
  postgresql:
    enabled: true
    version: "15"  # Override preset's default version
    port: 5433     # Use different port than default 5432
    # All other PostgreSQL settings from Django preset are preserved

# The Django preset still provides:
# - Redis configuration
# - Django-specific pip packages
# - Development environment variables
# - Port 8000 for Django dev server
```

#### 2. Port Modifications

Customize ports while inheriting other preset configurations:

```yaml
# vm.yaml - Custom ports for React development
project:
  name: "my-react-app"

ports:
  - 3010        # Custom React dev server port
  - 3011        # Custom port for API proxy
  - 8080        # Custom webpack dev server
  # This completely replaces the React preset's ports array
  # Original preset ports (3000, 3001, 5173) are not included

# To ADD ports to preset defaults, you need to specify all ports:
ports:
  - 3000        # React dev server (from preset)
  - 3001        # Alternative React port (from preset)  
  - 5173        # Vite dev server (from preset)
  - 3010        # Your custom port
  - 8080        # Your custom webpack port
```

#### 3. Package Customization

Add or replace packages from detected presets:

```yaml
# vm.yaml - Customize React preset packages
project:
  name: "my-react-app"

# Add additional npm packages to those from React preset
npm_packages:
  - create-react-app     # From React preset
  - react-router-dom     # From React preset
  - "@vitejs/plugin-react" # From React preset
  - vite                 # From React preset
  - react-testing-library # From React preset
  - jest                 # From React preset
  - "@storybook/react"   # Your addition
  - "eslint-config-airbnb" # Your addition
  - "styled-components"  # Your addition

# Add Python packages (even though this is primarily a React project)
pip_packages:
  - git-filter-repo      # From base preset
  - httpie              # From base preset
  - black               # Your addition for any Python scripts
  - requests            # Your addition
```

#### 4. Service Overrides

Disable or modify services from presets:

```yaml
# vm.yaml - Disable Redis from React preset, keep other settings
project:
  name: "my-simple-react-app"

services:
  redis:
    enabled: false  # Override React preset's Redis configuration
  # PostgreSQL remains disabled (React preset default)
  
# All other React preset benefits remain:
# - npm_packages for React development
# - Development environment variables
# - Other ports (3000, 3001, 5173)
```

#### 5. Multi-Preset Customization

When your project triggers multiple preset detection:

```yaml
# Project with both package.json (React) and requirements.txt (Django API)
# Detected as: "multi:react django"
# System loads: Base → React → Django → Your config

project:
  name: "fullstack-app"

# Override Django's default port (8000) since React uses 3000
services:
  postgresql:
    enabled: true
    port: 5432    # Keep PostgreSQL on standard port
  redis:
    enabled: true  # Both presets enable Redis, your config confirms it

ports:
  - 3000    # React frontend (from React preset)
  - 3001    # React dev tools (from React preset)
  - 5173    # Vite server (from React preset)
  - 8000    # Django API (from Django preset)
  - 8001    # Django alternative (from Django preset)
  - 5555    # Celery flower (from Django preset)
  - 4000    # Your custom API documentation port

environment:
  REACT_APP_API_URL: "http://localhost:8000"  # Connect React to Django
  DEBUG: "True"                               # From Django preset
  NODE_ENV: "development"                     # From React preset
```

### Creating Custom Presets

For advanced users who want to create reusable configurations:

#### 1. Creating a New Preset File

```yaml
# configs/presets/fastapi.yaml - Custom FastAPI preset
---
preset:
  name: "FastAPI Development"
  description: "Optimized for FastAPI Python development"

pip_packages:
  - fastapi
  - uvicorn[standard]
  - python-multipart
  - sqlalchemy
  - alembic
  - pytest
  - httpx  # For testing
  - python-decouple

ports:
  - 8000  # FastAPI default
  - 8001  # Alternative FastAPI port

services:
  postgresql:
    enabled: true
  redis:
    enabled: true

environment:
  PYTHONPATH: "/workspace"
  FASTAPI_ENV: "development"
```

#### 2. Using Custom Presets

```bash
# Force your custom preset
./vm.sh --preset fastapi create my-api

# Or reference in vm.yaml
```

```yaml
# vm.yaml
preset: "fastapi"  # Force this preset regardless of detection
project:
  name: "my-fastapi-app"
```

### Advanced Configuration Patterns

#### 1. Environment-Specific Configurations

```yaml
# vm.yaml - Different configs for different environments
project:
  name: "my-app"

# Base configuration that works with presets
services:
  postgresql:
    enabled: true
  redis:
    enabled: true

# Override for production-like local testing
vm:
  memory: 8192      # More memory than preset defaults
  cpus: 4           # More CPUs for performance testing

environment:
  NODE_ENV: "production"    # Override preset's development setting
  DEBUG: "False"            # Override Django preset's debug setting
```

#### 2. Selective Package Management

```yaml
# vm.yaml - Fine-grained package control
project:
  name: "my-custom-stack"

# Completely replace preset npm packages with your own selection
npm_packages:
  - "vite"          # Modern build tool instead of webpack
  - "typescript"    # Add TypeScript support
  - "vitest"        # Modern testing instead of jest
  - "@vitejs/plugin-react"

# Keep preset pip packages but add your own
pip_packages:
  - git-filter-repo # From base preset
  - httpie         # From base preset
  - poetry         # Your addition for dependency management
  - ruff           # Your addition for linting
```

#### 3. Service Configuration Overrides

```yaml
# vm.yaml - Advanced service customization
project:
  name: "my-app"

services:
  postgresql:
    enabled: true
    version: "15"
    port: 5432
    database: "my_custom_db"    # Override preset's default database name
    user: "dev_user"            # Custom user instead of preset default
    
  redis:
    enabled: true
    port: 6380                  # Non-standard port
    maxmemory: "256mb"          # Custom memory limit
    
  # Add a service not in any preset
  elasticsearch:
    enabled: true
    port: 9200
    version: "8.0"
```

### Troubleshooting Preset Issues

#### Common Problems and Solutions

**Problem**: Wrong preset detected
```bash
# Check what preset would be detected
./vm.sh --dry-run create test-vm

# Force a specific preset
./vm.sh --preset django create my-app

# Or specify in vm.yaml
preset: "django"
```

**Problem**: Preset overriding your settings
```yaml
# Ensure your vm.yaml values are correct - they always win
# Check for typos in property names (e.g., 'port' vs 'ports')

# Incorrect - won't override preset
services:
  postgresql:
    port: 5433  # Wrong property name

# Correct - will override preset  
services:
  postgresql:
    enabled: true
    port: 5433
```

**Problem**: Package conflicts between presets
```yaml
# For multi-preset projects, explicitly specify packages
npm_packages:
  # List ALL packages you want, from both presets and custom
  - "react"           # From React preset
  - "django"          # This would be in pip_packages normally
  - "your-package"    # Your addition

pip_packages:
  - "django"          # From Django preset  
  - "react"           # This would be in npm_packages normally
  - "your-package"    # Your addition
```

**Problem**: Service configuration conflicts
```yaml
# When multiple presets configure the same service differently,
# be explicit about what you want

services:
  postgresql:
    enabled: true     # Explicitly enable
    version: "14"     # Explicitly choose version
    port: 5432        # Explicitly choose port
    # This resolves any conflicts between preset defaults
```

#### Debugging Configuration Merging

```bash
# Enable debug mode to see configuration merging
VM_DEBUG=true ./vm.sh create my-app

# Check what configuration would be generated
./vm.sh --dry-run create my-app

# Validate your vm.yaml before use
./validate-config.sh --validate vm.yaml

# Extract just the defaults to see baseline
./validate-config.sh --extract-defaults vm.schema.yaml
```

### Best Practices for Customization

1. **Start Small**: Begin with minimal overrides and add customizations incrementally
2. **Be Explicit**: When overriding arrays (ports, packages), include all values you want
3. **Document Overrides**: Comment your vm.yaml to explain why you're overriding preset defaults
4. **Test Changes**: Use `--dry-run` to preview configuration before creating VMs
5. **Validate Frequently**: Run `./validate-config.sh` to catch configuration errors early
6. **Use Environment Variables**: For secrets and environment-specific values
7. **Keep Presets Enabled**: Only use `--no-preset` when you truly need full manual control

### Integration with Development Workflow

The customization system is designed to support various development workflows:

**Solo Development**: Override preset defaults for your specific needs
```yaml
# vm.yaml - Personal development preferences
project:
  name: "my-project"
  
ports:
  - 3000
  - 8080    # Your preferred alternative port

npm_packages:
  - "your-preferred-packages"
```

**Team Development**: Create team-specific base configurations
```yaml
# team-base.yaml - Shared team configuration
project:
  workspace_path: "/workspace"
  
vm:
  memory: 4096  # Team standard
  cpus: 2
  
environment:
  TEAM: "frontend-team"
  NODE_ENV: "development"
```

**CI/CD Integration**: Environment-specific overrides for testing
```yaml
# ci-vm.yaml - CI-specific configuration
vm:
  memory: 2048   # Minimal resources for CI
  cpus: 1

services:
  postgresql:
    enabled: true
  redis:
    enabled: false  # Not needed for unit tests
```

## Conclusion

The preset system makes VM configuration faster and more consistent while maintaining full flexibility. Use automatic detection for most cases, force specific presets when needed, or disable entirely for complete control. The system is designed to enhance, not restrict, your development workflow.

**Remember**: The customization system follows the principle of "smart defaults with easy overrides." Presets provide excellent starting points, but your `vm.yaml` configuration always has the final say in how your development environment is configured.