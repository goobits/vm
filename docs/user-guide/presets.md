# Presets

Presets are reusable configuration overlays for common project types.

```bash
vm config preset --list
vm config preset nodejs
vm config preset python,postgres
vm run linux as app
```

For a Linux Tart VM with Docker Engine:

```bash
vm config preset vibe-tart
vm config profile set tart
vm ssh
```

`vibe-tart` already uses `tart` as its default profile; the profile command is
only needed when switching back from another profile. Its `macos` profile is an
explicit Colima/QEMU fallback for macOS-only tooling. Remove that optional
profile from a project with:

```bash
vm config unset profiles.macos
```

Preset-backed `unset` operations first materialize the effective preset, so the
removed field stays removed in `vm.yaml` and in subsequent `vm config show`
output. Vibe presets do not add a named network unless one is configured.

Provider-native base workflows live under `system base`:

```bash
vm system base build vibe --provider docker
vm system base build vibe --provider tart
vm system base build vibe --provider tart --guest-os macos
```

New Docker environments verify that `@vibe-image` contains Codex's complete
standalone runtime before deriving the project image. If an older cached base
is incomplete, creation stops with the exact non-destructive base-build command
instead of opening a partially working Codex session.

When the versioned Linux vibe base is missing locally, environment creation
pulls it from GHCR into a versioned local cache. If that image is unavailable,
`vm` builds the same cache locally. This applies when creation starts through
`vm ssh` as well as `vm run`. Use `vm system base build` directly to
deliberately rebuild a base.

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
