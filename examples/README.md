# Examples

Examples use the current CLI.

```bash
vm start app
vm shell app
vm exec --env app -- npm test
vm snapshots create configured --env app
vm snapshots restore configured --env app
vm package app --output app.tar.gz
```

Base-image workflows live under `system`:

```bash
vm system base build vibe --provider docker
vm system base validate vibe --provider docker
```
