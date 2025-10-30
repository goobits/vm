# Vibe Development Base Image

Optimized Docker base image for fast snapshot-based development with all your vibe preset tools pre-installed.

## What's Included

### 🎯 Core Tools
- **System utilities:** tree, ripgrep, unzip, htop, jq, tmux
- **Build tools:** gcc, g++, make, build-essential
- **Node.js** (latest LTS) with npm
- **Python** (latest stable from deadsnakes PPA) with pip
- **Rust** (latest stable) with Cargo

> **Note:** This image always installs the latest stable versions at build time. Rebuild periodically to get updates.

### 🤖 AI CLI Tools
- `@anthropic-ai/claude-code` - Claude Code CLI
- `@google/gemini-cli` - Gemini CLI
- Pre-configured aliases: `claudeyolo`, `geminiyolo`

### 🎭 Testing & Automation
- **Playwright** (Node.js & Python)
- **Chromium** browser pre-installed
- pytest + pytest-playwright

### 🛠️ Development Tools
- **Node:** prettier, eslint, typescript, ts-node, nodemon, npm-check-updates
- **Python:** claudeflow
- **Rust:** cargo-watch, cargo-edit, cargo-audit, cargo-outdated, rustfmt, clippy, rust-analyzer

## Build the Base Image

```bash
# Standard build (always gets latest stable of everything)
docker build -f Dockerfile.vibe -t vibe-base:latest .

# Build with custom user ID (recommended to match host)
docker build -f Dockerfile.vibe \
  --build-arg USER_UID=$(id -u) \
  --build-arg USER_GID=$(id -g) \
  -t vibe-base:latest .

# Rebuild to get latest versions (do this periodically)
docker build --no-cache -f Dockerfile.vibe -t vibe-base:latest .
```

## Use as Base Image

### Quick Run
```bash
docker run -it --rm \
  -v $(pwd):/workspace \
  -e ANTHROPIC_API_KEY="your-key" \
  vibe-base:latest
```

### Extend for Specific Presets

**Example: Django Project (FROM vibe-base)**
```dockerfile
FROM vibe-base:latest

USER root

# Add Django-specific packages
RUN apt-get update && apt-get install -y \
    postgresql-client \
    && rm -rf /var/lib/apt/lists/*

USER developer

# Install Django packages
RUN pip3 install \
    django>=4.0 \
    psycopg2-binary \
    djangorestframework \
    celery

EXPOSE 8000
CMD ["python3", "manage.py", "runserver", "0.0.0.0:8000"]
```

**Example: React Project (FROM vibe-base)**
```dockerfile
FROM vibe-base:latest

USER developer

# Install React-specific tools
RUN npm install -g \
    create-react-app \
    vite \
    @vitejs/plugin-react

# Install testing tools
RUN npm install -g \
    @testing-library/react \
    jest

EXPOSE 3000 5173
CMD ["npm", "start"]
```

**Example: Rust Project (FROM vibe-base)**
```dockerfile
FROM vibe-base:latest

USER developer

# Rust is already installed! Just add project-specific tools
RUN cargo install \
    cargo-nextest \
    cargo-expand \
    wasm-pack

EXPOSE 8080
CMD ["cargo", "run"]
```

## Docker Compose Example

```yaml
version: '3.8'

services:
  dev:
    image: vibe-base:latest
    volumes:
      - .:/workspace
    environment:
      - ANTHROPIC_API_KEY=${ANTHROPIC_API_KEY}
      - GEMINI_API_KEY=${GEMINI_API_KEY}
    ports:
      - "3000-3010:3000-3010"
    working_dir: /workspace
    command: /bin/bash
    stdin_open: true
    tty: true
```

## Snapshot Workflow

### 1. Build base image once
```bash
docker build -f Dockerfile.vibe -t vibe-base:latest .
```

### 2. Create snapshots for different projects
```bash
# Start container for React project
docker run -it --name react-dev vibe-base:latest
# Inside container: install react-specific stuff
npm install -g create-react-app vite

# Commit snapshot
docker commit react-dev vibe-react:snapshot

# Start container for Django project
docker run -it --name django-dev vibe-base:latest
# Inside container: install django-specific stuff
pip3 install django djangorestframework

# Commit snapshot
docker commit django-dev vibe-django:snapshot
```

### 3. Use snapshots for instant startup
```bash
docker run -it --rm -v $(pwd):/workspace vibe-react:snapshot
docker run -it --rm -v $(pwd):/workspace vibe-django:snapshot
```

## Build Arguments

| Argument | Default | Description |
|----------|---------|-------------|
| `USER_NAME` | `developer` | Username in container |
| `USER_UID` | `1000` | User ID (match host) |
| `USER_GID` | `1000` | Group ID (match host) |

**Language Versions:** Automatically installs latest stable at build time
- **Node.js:** Latest LTS from NodeSource
- **Python:** Latest stable from deadsnakes PPA
- **Rust:** Latest stable from rustup

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `TZ` | `America/Los_Angeles` | Timezone |
| `NODE_ENV` | `development` | Node environment |
| `PYTHONDONTWRITEBYTECODE` | `1` | Disable .pyc files |
| `ANTHROPIC_API_KEY` | - | Claude API key |
| `GEMINI_API_KEY` | - | Gemini API key |

## Healthcheck

The image includes a healthcheck that verifies:
- Node.js is available
- Python 3 is available
- Cargo/Rust is available
- Claude CLI is installed

## Image Size Optimization

This base image is optimized for layer caching:
1. System packages (changes rarely) → cached
2. Language runtimes (changes rarely) → cached
3. Global tools (changes rarely) → cached
4. User setup (changes rarely) → cached
5. Your code (changes often) → mounted volume

## Tips

1. **Rebuild periodically to get latest versions:**
   ```bash
   # Full rebuild with latest stable versions
   docker build --no-cache -f Dockerfile.vibe -t vibe-base:latest .
   ```

2. **Match host UID/GID** to avoid permission issues:
   ```bash
   docker build -f Dockerfile.vibe \
     --build-arg USER_UID=$(id -u) \
     --build-arg USER_GID=$(id -g) \
     -t vibe-base:latest .
   ```

3. **Cache dependencies for faster rebuilds:**
   ```yaml
   volumes:
     - .:/workspace
     - node_modules:/workspace/node_modules
     - pip_cache:/home/developer/.cache/pip
     - cargo_registry:/root/.cargo/registry
     - cargo_git:/root/.cargo/git
   ```

4. **Share API keys securely:**
   ```bash
   docker run -it --rm \
     -v $(pwd):/workspace \
     --env-file .env \
     vibe-base:latest
   ```

## License

MIT
