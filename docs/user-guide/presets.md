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
vm system base validate vibe --provider all
```

Provider overrides remain advanced routing controls:

```bash
vm run linux as app --provider docker
vm run linux as isolated --provider tart
```
