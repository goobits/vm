# Presets

Presets are reusable configuration overlays for common project types.

```bash
vm config preset --list
vm config preset nodejs
vm config preset python,postgres
vm run linux as app
```

Provider-native base workflows live under `system base`:

```bash
vm system base build vibe --provider docker
vm system base build vibe --provider tart
vm system base build vibe --provider tart --guest-os macos
vm system base validate vibe --provider all
```

When the versioned Linux vibe base is missing locally, `vm create` pulls it from
GHCR into a versioned local cache. If that image is unavailable, `vm` builds the
same cache locally. Use `vm system base build` directly to deliberately rebuild
a base.

Set `tart.storage_path` to place Tart bases and environments on another disk:

```bash
mkdir -p /Volumes/ExternalSSD/Tart
vm config set tart.storage_path /Volumes/ExternalSSD/Tart
```

Provider overrides remain advanced routing controls:

```bash
vm run linux as app --provider docker
vm run linux as isolated --provider tart
```
