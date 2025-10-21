# Quick Start: Custom Base Images

Get up and running with custom base images in 5 minutes.

## Step 1: Choose Your Base Image

We provide two options:

- **`super-cool-vibes.dockerfile`** - Ultimate dev environment (Playwright + AI tools + everything) ~2.5GB
- **`minimal-node.dockerfile`** - Lightweight (Just Node.js + essentials) ~500MB

## Step 2: Build It

```bash
# Navigate to examples directory
cd examples/base-images

# Build Super Cool Vibes (recommended)
./build.sh supercool

# Or build minimal version
./build.sh minimal
```

⏱️ **First build takes 5-15 minutes** (downloading & installing everything)

## Step 3: Use It in Your Project

Edit your project's `vm.yaml`:

```yaml
# vm.yaml
vm:
  box_name: super-cool-vibes:latest  # ← Use your custom image

# Everything else works normally
ports:
  frontend: 3000

services:
  postgresql:
    enabled: true
```

## Step 4: Create Your VM

```bash
cd /path/to/your/project
vm create
```

🚀 **VM creation is now much faster** - Playwright/Chromium already installed!

## Step 5 (Optional): Enable Docker Registry Caching

Make subsequent VMs even faster:

```bash
vm config set services.docker_registry.enabled true --global
```

Now all VMs will share cached images - nearly instant startup!

## Customize Your Own

Copy an example and modify:

```bash
cp super-cool-vibes.dockerfile my-custom.dockerfile
# Edit my-custom.dockerfile - add your tools
docker build -f my-custom.dockerfile -t my-custom-base:latest .
```

See [README.md](./README.md) for full documentation and best practices.
