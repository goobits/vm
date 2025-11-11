# Base Images

Pre-built Docker images with heavy dependencies for faster VM creation.

## Why Base Images?

Installing large dependencies like Playwright/Chromium or Python ML libraries takes 5-10 minutes on every `vm create`. Base images pre-install these dependencies, reducing VM creation to seconds.

## Available Images

### playwright-chromium.dockerfile

Pre-installs Playwright with Chromium browser for E2E testing.

**Use when**: Running Playwright, Puppeteer, or Selenium tests

**Build**:
```bash
docker build -f examples/base-images/playwright-chromium.dockerfile -t my-playwright-base .
```

**Use in vm.yaml**:
```yaml
vm:
  box: my-playwright-base
```

**Time saved**: ~5 minutes per VM creation

### python-ml.dockerfile

Pre-installs NumPy, Pandas, Scikit-learn, Jupyter for data science.

**Use when**: Machine learning, data analysis, scientific computing

**Build**:
```bash
docker build -f examples/base-images/python-ml.dockerfile -t my-python-ml-base .
```

**Use in vm.yaml**:
```yaml
vm:
  box: my-python-ml-base
```

**Time saved**: ~8 minutes per VM creation

### node-heavy.dockerfile

Pre-installs common Node.js dependencies and build tools.

**Use when**: Large Node projects with many dependencies

**Build**:
```bash
docker build -f examples/base-images/node-heavy.dockerfile -t my-node-base .
```

**Use in vm.yaml**:
```yaml
vm:
  box: my-node-base
```

**Time saved**: ~3 minutes per VM creation

## Creating Custom Base Images

1. Start with a Dockerfile installing your heavy dependencies
2. Build the image: `docker build -f your.dockerfile -t your-base-name .`
3. Reference in vm.yaml: `vm.box: your-base-name`
4. Share with team by publishing to Docker Hub or private registry

## Best Practices

- Keep base images focused (one use case per image)
- Update base images regularly for security patches
- Use multi-stage builds to minimize image size
- Tag images with versions for reproducibility
- Document installed versions in Dockerfile comments
