# Proposal 04: VM Configuration Profiles

**Status:** Draft
**Date:** 2026-01-11
**Objective:** Enable multiple runtime configurations (e.g., `docker` vs `tart`, `dev` vs `ci`) within a single `vm.yaml` using a DRY, inheritance-based profile system.

## TL;DR
Introduce a `profiles` key in `vm.yaml`. Users define a **base configuration** (shared settings) and named **profiles** (overrides). The CLI merges specific profiles over the base at runtime via `vm start --profile <name>`.

## Tree Diff (Implementation Scope)

```diff
 /workspace
 ├── configs/schema/
 │   └── vm.schema.yaml          # + definitions: { profile: ... }, properties: { profiles: ... }
 ├── rust/vm-config/src/
 │   ├── lib.rs                  # + struct VmConfig { ..., profiles: HashMap<String, Profile> }
 │   └── merge.rs                # + fn merge_profile(base: &Config, profile: &Profile) -> Config
 ├── rust/vm-cli/src/commands/
 │   └── up.rs                   # + arg: --profile <name>
 └── docs/user-guide/
     └── configuration.md        # + Section: "Using Profiles"
```

## Schema Design (SOLID/DRY)

**Principle:** The `Base` config acts as an abstract class. `Profiles` are concrete implementations that extend and override the base.

```yaml
# vm.yaml (Proposed)

# --- BASE CONFIGURATION (Shared) ---
project:
  name: my-app
  workspace_path: /workspace

# Common services for ALL profiles
services:
  postgresql: { enabled: true }
  redis: { enabled: true }

# --- PROFILES (Specific Implementations) ---
profiles:
  # Usage: vm start (Default if "default" exists, else Base)
  default:
    provider: docker
    vm:
      memory: 4096
      cpus: 4

  # Usage: vm start --profile tart (or -p tart)
  tart:
    provider: tart
    vm:
      memory: 16384
      cpus: 8
    services:
      gpu: true  # Specific to Tart/macOS
      docker: { enabled: true } # Tart needs nested docker
```

## Implementation Plan

### 1. Schema Update (`configs/schema/vm.schema.yaml`)
- Define `Profile` schema (subset of `VmConfig` excluding `project.name` and other immutable fields).
- Add `profiles` map to root object.

### 2. Config Crate (`rust/vm-config`)
- **Deserialization:** Update `VmConfig` struct to include `profiles`.
- **Logic (`Merge Strategy`):** Implement a "Deep Merge" trait.
  - Arrays (e.g., `npm_packages`): **Union** (Base + Profile).
  - Objects (e.g., `services`): **Recursive Merge** (Profile overrides Base keys).
  - Primitives (e.g., `memory`): **Replace** (Profile overwrites Base).

### 3. CLI Update (`rust/vm-cli`)
- Add `--profile, -p <NAME>` global flag.
- During initialization:
  1. Load `vm.yaml`.
  2. If `--profile` is set, look up key.
  3. Clone Base -> Apply Merge -> Return Final Config.
  4. If profile missing -> Error.

## Future Extensibility
- **Composition:** Allow profiles to inherit from others (e.g., `ci-tart` extends `tart`).
- **Auto-Activation:** Select profile based on host OS or hostname automatically.
