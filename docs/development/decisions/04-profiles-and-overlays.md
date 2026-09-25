# Profiles And Overlays

Profiles remain configuration-level overlays in `vm.yaml`. Profiles select configuration variants for declared environments.

```bash
vm create backend --provider docker --image ubuntu:24.04
vm start backend
vm start backend --profile secure
```

Profiles are useful for shared config variants and resource limits. Existing environments retain their recorded provider.
