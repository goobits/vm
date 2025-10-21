# Custom Base Images

This directory contains example Dockerfiles for creating custom base images that can be reused across multiple VMs. Base images allow you to pre-install heavy dependencies (like Playwright, Chromium, or other large packages) once, then reuse them across all your projects for faster VM creation.

## Why Use Custom Base Images?

- **Faster VM creation** - Heavy dependencies are pre-installed
- **Consistent environments** - Same base across all projects
- **Bandwidth savings** - Download large packages once
- **Works with Docker registry caching** - Combine with global Docker registry for maximum speed

## Available Examples

### `super-cool-vibes.dockerfile` 🎭
**The ultimate development environment!** Based on the "vibe" preset with Playwright, Chromium, AI CLI tools (Claude, Gemini, Codex), and modern dev utilities. This is the kitchen-sink image for serious development.

**Includes:**
- Playwright + Chromium (browser automation)
- AI CLI tools (@anthropic-ai/claude-code, @google/gemini-cli, @openai/codex)
- Python AI SDKs (anthropic, openai, google-generativeai)
- Modern CLI tools (tree, ripgrep, htop, jq, tmux)
- TypeScript ecosystem (typescript, ts-node, tsx)
- Code quality (prettier, eslint)
- Dev utilities (nodemon, vitest, jest)
- Package managers (pnpm, yarn)

**Size:** ~2.5GB
**Build time:** 10-15 minutes (first time only!)

**Use cases:**
- Full-stack development with AI assistance
- E2E testing with Playwright
- Browser automation & web scraping
- Projects using AI APIs
- Modern web development with all the tools

### `minimal-node.dockerfile` ⚡
Lightweight base with just Node.js and essential tools. Fast to build, small image size.

**Includes:**
- Node.js 22 LTS
- pnpm, TypeScript
- git, curl, ca-certificates

**Size:** ~500MB
**Build time:** 2-3 minutes

**Use cases:**
- Simple Node.js apps
- Microservices
- Projects with minimal dependencies
- Quick prototypes

## How to Use

### 1. Build the Base Image

```bash
# Build from the examples directory
cd examples/base-images

# Build Super Cool Vibes (recommended)
./build.sh supercool

# Or build manually
docker build -f super-cool-vibes.dockerfile -t super-cool-vibes:latest .

# Or build the minimal version
./build.sh minimal
```

### 2. Use in Your Project

Update your project's `vm.yaml` to use your custom base image:

```yaml
# vm.yaml
vm:
  box_name: super-cool-vibes:latest  # Your custom base image

# All other settings work normally
ports:
  frontend: 3000
  backend: 3001

services:
  postgresql:
    enabled: true

npm_packages:
  - react  # Project-specific packages
  - vite
```

### 3. Speed Up with Docker Registry Caching

Enable the global Docker registry cache for even faster pulls:

```bash
# Enable once, benefits all VMs
vm config set services.docker_registry.enabled true --global
```

Now your custom base image will be cached locally, making subsequent VM creations nearly instant!

## Creating Your Own Base Image

### Basic Template

```dockerfile
FROM ubuntu:24.04

LABEL description="My Custom Base Image"
LABEL version="1.0"

# Install heavy/slow dependencies here
RUN apt-get update && apt-get install -y \
    your-packages-here \
    && rm -rf /var/lib/apt/lists/*

# Install language-specific tools
RUN npm install -g your-global-packages

# Set up environment
ENV YOUR_ENV_VAR=value
WORKDIR /workspace
```

### Best Practices

1. **Only include common dependencies** - Don't add project-specific packages
2. **Clean up apt cache** - Use `&& rm -rf /var/lib/apt/lists/*` after apt-get
3. **Use labels** - Document what your image contains
4. **Version your images** - Tag with versions (e.g., `my-base:v1.0`, `my-base:latest`)
5. **Keep it focused** - Create multiple specialized bases rather than one giant image

### Example: Building Multiple Versions

```bash
# Build and tag multiple versions
docker build -f playwright-chromium.dockerfile \
  -t my-playwright-base:latest \
  -t my-playwright-base:v1.0 \
  .

# Old projects can stick with v1.0
vm:
  box_name: my-playwright-base:v1.0

# New projects use latest
vm:
  box_name: my-playwright-base:latest
```

## Image Management

### List Your Custom Images

```bash
docker images | grep -E "my-.*-base"
```

### Remove Old Images

```bash
docker rmi my-playwright-base:v1.0
```

### Share Images with Your Team

```bash
# Save to file
docker save my-playwright-base:latest -o my-base.tar

# Load on another machine
docker load -i my-base.tar
```

Or push to a registry:

```bash
# Tag for registry
docker tag my-playwright-base:latest registry.example.com/my-playwright-base:latest

# Push
docker push registry.example.com/my-playwright-base:latest
```

## Tips & Tricks

### Combine Base Images with Presets

You can use custom base images alongside VM presets for maximum flexibility:

```yaml
# vm.yaml
preset: nextjs  # Use Next.js preset for config
vm:
  box_name: my-playwright-base:latest  # But use custom base image
```

### Layer Your Images

Create specialized images on top of your base:

```dockerfile
# my-react-base.dockerfile
FROM my-dev-base:latest

# Add React-specific tooling
RUN npm install -g create-react-app vite

LABEL description="React development base"
```

### Check Image Size

```bash
docker images my-playwright-base:latest --format "{{.Size}}"
```

Keep images reasonably sized - aim for under 3GB if possible.

## Troubleshooting

### Image Not Found

Make sure the image is built locally:
```bash
docker images | grep my-base
```

### VM Still Slow to Create

- Check if Docker registry caching is enabled: `vm config get services.docker_registry --global`
- Verify image is actually being used: Check `vm.yaml` has correct `box_name`
- First VM creation will always be slower (pulling base image)

### Build Failures

- Check Dockerfile syntax
- Ensure base image exists: `docker pull ubuntu:24.04`
- Review build logs for specific errors

## See Also

- [VM Configuration Guide](../../docs/user-guide/configuration.md)
- [Examples Guide](../../docs/getting-started/examples.md)
- [Docker Provider Documentation](../../docs/providers/docker.md)
