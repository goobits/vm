# Proposal 05: Codebase Organization And De-duplication

**Status:** Draft
**Date:** 2026-02-18
**Objective:** Reduce duplication and confusion in repo layout (UI + docs + repeated Rust logic) while preserving the existing layered Rust workspace architecture.

## TL;DR

1. Keep the Rust workspace **layered** (crates grouped by Foundation/Config/Provider/App/Service/Utility) as documented in `rust/ARCHITECTURE.md`.
2. Establish **single sources of truth**:
   - UI source lives in `site/` only (remove tracked duplicates in `rust/site/`).
   - Docs source lives in `docs/` only; `site/src/routes/docs-md/` becomes generated (or removed in favor of runtime loading).
3. Reduce repeated Rust code by extracting shared helpers (notably CLI “print info blocks” and provider compose regeneration).
4. Improve clone reports by ignoring generated directories like `site/.svelte-kit/`.

## Problem

The repo is conceptually organized, but there are a few high-cost forms of duplication:

- **UI code is checked in twice** (`site/**` and `rust/site/**`), despite a changelog note that it was consolidated.
- **Docs are duplicated** (`docs/**` and `site/src/routes/docs-md/**`).
- There is meaningful **Rust code duplication** (small repeated blocks across command handlers and provider lifecycle modules).
- Clone detection currently reports many “clones” that are either intended (platform-specific variants) or generated output.

These increase maintenance cost, create drift risk, and make “what is the source of truth?” unclear to contributors.

## Non-Goals

- Changing the crate layering model (this proposal assumes the layered workspace is intentional and should remain).
- Large-scale renames or wholesale folder restructures inside each crate.
- Rewriting the docs or the UI; this proposal is about layout and duplication reduction.

## Current Observations (As Of 2026-02-18)

### Workspace organization

- Rust is explicitly layered in `rust/Cargo.toml` and described in `rust/ARCHITECTURE.md`.
- Within crates, the directory structure is generally domain/feature-cohesive:
  - `rust/vm-provider/src/{docker,podman,tart}/...`
  - `rust/vm/src/commands/{snapshot,vm_ops,db}/...`
  - `rust/vm-package-server/src/{registry,validation,client_ops}/...`

### Exact duplicates tracked in git

- UI duplication:
  - `rust/site/src/lib/api/operations.ts` == `site/src/lib/api/operations.ts`
  - `rust/site/src/lib/api/snapshots.ts` == `site/src/lib/api/snapshots.ts`
  - `rust/site/src/lib/api/workspaces.ts` == `site/src/lib/api/workspaces.ts`
  - `rust/site/src/lib/components/OperationsHistory.svelte` == `site/src/lib/components/OperationsHistory.svelte`
  - `rust/site/src/lib/components/SnapshotManager.svelte` == `site/src/lib/components/SnapshotManager.svelte`
  - `rust/site/src/lib/components/WorkspaceList.svelte` == `site/src/lib/components/WorkspaceList.svelte`
  - `rust/site/src/lib/types/workspace.ts` == `site/src/lib/types/workspace.ts`
  - plus `rust/site/src/routes/**` overlap with `site/src/routes/**`

- Docs duplication:
  - `docs/**.md` duplicates exist in `site/src/routes/docs-md/**.md` (verbatim copies).

## Proposal

### A. Single UI Source: `site/` Only

**Decision:** Delete tracked `rust/site/**` sources and keep `site/**` as the only UI codebase.

Rationale:
- The root already has a dedicated `site/` project with its own `package.json`.
- The files in `rust/site/**` are a small subset and are exact duplicates, indicating historical movement.
- Having two copies invites drift and confuses contributors.

Implementation notes:
- Remove tracked files under `rust/site/**`.
- Verify no build scripts, docs, or Rust binaries reference `rust/site/` paths.

### B. Single Docs Source: `docs/` Only

Pick one of these approaches (recommendation first):

1. **(Recommended) Generate `site/src/routes/docs-md/**` from `docs/**`**
   - Add a simple sync script (copy-only) that runs in CI and optionally pre-commit.
   - Mark `site/src/routes/docs-md/**` as generated: ensure it is either ignored by git or owned by a generator workflow.

2. Load docs at runtime/build time without duplicating them in-tree
   - e.g. configure the Svelte site to import markdown from `docs/**` directly (if tooling allows), or to fetch it from a static mount.

Rationale:
- Avoid drift between two copies of the same documents.
- Keep authoring location clear (`docs/`).

### C. Reduce Rust Duplication (Targeted Refactors)

This is a “pay down the obvious duplicates” pass, not a big redesign.

1. Extract the repeated “VM info block” printing logic (resources/services/ports) used by multiple `vm` commands.
   - Likely location: `rust/vm/src/cli/` or `rust/vm/src/commands/vm_ops/helpers.rs` (depending on existing conventions).
   - Goal: one helper that takes `(config, vm_name/container_name, instance, ...)` and prints the consistent block.

2. In `vm-provider` docker lifecycle ops, consolidate duplicated compose regeneration logic used by start/restart-with-context.
   - Create a helper like `regenerate_compose(context) -> Result<ComposeOperations>` or similar.

Rationale:
- Reduces bug risk (formatting or behavior changes need to happen once).
- Improves readability of command handlers and lifecycle ops.

### D. Clone Detection Hygiene

**Decision:** Update clone detection ignores so reports focus on actionable duplication.

- Add `**/.svelte-kit/**` to `.jscpd.json` ignore list (generated output).
- Optionally ignore `site/src/routes/docs-md/**` if it becomes generated.

## Tree Diff (Implementation Scope)

```diff
/workspace
├── proposals/next/
│   └── 05_codebase_organization_dedup.proposal.md
├── site/
│   └── src/routes/docs-md/              # becomes generated OR removed (see choice B)
├── docs/                                # becomes the only docs source of truth
├── rust/
│   ├── vm/                              # refactor repeated printing into helper
│   └── vm-provider/                     # factor compose regeneration helper
├── .jscpd.json                           # ignore generated outputs
└── rust/site/                            # remove tracked duplicates (source no longer lives here)
```

## Implementation Plan

1. UI consolidation
   - Confirm nothing references `rust/site/**`.
   - Delete tracked `rust/site/**`.

2. Docs consolidation (choose one option from section B)
   - Implement generator/sync or direct-import solution.
   - Update contributor docs to state the authoring location.

3. Rust de-dup refactors
   - Extract “info block” printing helper.
   - Consolidate compose regeneration helper.
   - Run `cargo test` for affected crates (`vm`, `vm-provider`).

4. Tooling cleanup
   - Update `.jscpd.json` ignore list.
   - Ensure clone reports don’t include generated output.

## Acceptance Criteria

- `git ls-files 'rust/site/**'` returns 0 tracked files.
- Docs authoring location is unambiguous:
  - Either `site/src/routes/docs-md/**` is generated, or it no longer exists and the site loads from `docs/**`.
- `npx -y jscpd -c .jscpd.json` produces a clone report without generated-directory noise (`.svelte-kit`, etc.).
- `cargo test -p vm -p vm-provider` passes.

## Risks / Tradeoffs

- If `rust/site/**` is still expected by a build step, removing it could break older workflows.
  - Mitigation: grep references + update docs/scripts prior to deletion.
- If `site/src/routes/docs-md/**` is currently hand-edited, switching it to generated will require a workflow change.
  - Mitigation: document the new source of truth clearly and provide a simple sync command.

## Open Questions

- Should `site/src/routes/docs-md/**` be generated and committed, or generated and ignored?
- Do we want a “docs engine” path that reads directly from `docs/**` to avoid copies entirely?

