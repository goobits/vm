# Examples

Examples use the v5 humane CLI surface.

```bash
vm run linux as app
vm shell app
vm exec app -- npm test
vm save app as configured
vm revert app configured
vm package app --output app.tar.gz
```

Base-image workflows live under `system`:

```bash
vm system base build vibe --provider docker
vm system base validate vibe --provider docker
```
