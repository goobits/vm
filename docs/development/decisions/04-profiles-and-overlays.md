# Profiles And Overlays

Profiles remain configuration-level overlays in `vm.yaml`. The public v5 CLI is intent-first, so users select environment kinds and names rather than provider-first commands.

```bash
vm run linux as backend
vm run mac as xcode
vm run linux as secure --provider tart
```

Provider overrides are advanced routing controls. Profiles are still useful for shared config variants, resource limits, and provider-specific settings.
