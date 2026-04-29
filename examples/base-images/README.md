# Base Images

Base image workflows live under `vm system base`.

```bash
vm system base build vibe --provider docker
vm system base build vibe --provider tart
vm system base validate vibe --provider all
```

Use base images when large dependencies should be prepared once and reused by many environments.
