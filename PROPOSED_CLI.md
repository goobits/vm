# Proposed CLI for Base Images + Snapshots

## The Problem We're Solving

Users want to:
1. Build Dockerfile.vibe ONCE
2. Share it across machines/team
3. Use instantly with `vm.box: @vibe-base`

Current CLI doesn't have a clear path for this.

## Proposed Solution: Minimal CLI Changes

### 1. Enhance `vm create` with `--snapshot` flag

```bash
# Build and auto-save as reusable base image
vm create --snapshot @vibe-base

# Optionally specify Dockerfile directly (no vm.yaml needed)
vm create --snapshot @vibe-base --from ./Dockerfile.vibe

# Creates reusable base image at:
# ~/.config/vm/snapshots/global/vibe-base/
```

### 2. Add `vm snapshot export/import`

```bash
# Export to portable file
vm snapshot export @vibe-base

# Outputs: vibe-base.snapshot.tar.gz (2-3 GB)

# Import on another machine
vm snapshot import vibe-base.snapshot.tar.gz

# Now can use with vm.box: @vibe-base
```

### 3. Enhance `vm snapshot list` to show type

```bash
vm snapshot list

# Output:
# TYPE    NAME              CREATED           SIZE    DESCRIPTION
# base    @vibe-base        2024-10-30 12:34  2.1 GB  Base vibe image
# project my-project-snap   2024-10-30 14:20  1.8 GB  Before refactor
```

## Complete Workflow Example

### Machine A: Create and Share Base Image

```bash
# Step 1: Create vm.yaml for base image build
cat > vm.yaml <<EOF
provider: docker
vm:
  box: ./Dockerfile.vibe
project:
  name: vibe-base-build
EOF

# Step 2: Build and save as reusable snapshot
vm create --snapshot @vibe-base

# Step 3: Export for sharing
vm snapshot export @vibe-base
# Creates: vibe-base.snapshot.tar.gz

# Step 4: Share file (Dropbox, S3, USB, etc.)
aws s3 cp vibe-base.snapshot.tar.gz s3://my-bucket/
```

### Machine B: Import and Use

```bash
# Step 1: Import
vm snapshot import vibe-base.snapshot.tar.gz

# Step 2: Use in any project
cat > vm.yaml <<EOF
provider: docker
vm:
  box: @vibe-base  # ← Instant startup!
project:
  name: my-awesome-project
ports:
  web: 3000
EOF

# Step 3: Create (fast!)
vm create
# ✓ Starts in 5-10 seconds instead of 10-15 minutes
```

## Implementation Details

### Snapshot Types

**Base Snapshots** (global, reusable)
- Stored in: `~/.config/vm/snapshots/global/<name>/`
- Referenced with: `@<name>` prefix
- Contains: Image only, no volumes
- Use case: Reusable base images

**Project Snapshots** (project-specific)
- Stored in: `~/.config/vm/snapshots/<project>/<name>/`
- Referenced with: `<name>` (no @)
- Contains: Image + volumes + state
- Use case: Save/restore project state

### CLI Command Summary

```bash
# Create reusable base from Dockerfile
vm create --snapshot @vibe-base [--from ./Dockerfile]

# Create project state snapshot (existing)
vm snapshot create my-state [--quiesce]

# List all snapshots
vm snapshot list [--type base|project]

# Export snapshot (base or project)
vm snapshot export @vibe-base [--output file.tar.gz]
vm snapshot export my-state [--output file.tar.gz]

# Import snapshot
vm snapshot import file.tar.gz [--name override]

# Restore (existing)
vm snapshot restore @vibe-base    # For base images
vm snapshot restore my-state      # For project snapshots

# Delete (existing)
vm snapshot delete @vibe-base
```

## Key Design Decisions

### 1. `@` Prefix for Base Snapshots
- Clear distinction: `@vibe-base` = base image, `my-state` = project state
- Consistent with existing `vm.box: @snapshot` syntax
- Easy to recognize in CLI output

### 2. Global vs Project Storage
- Base snapshots: Always global (`~/.config/vm/snapshots/global/`)
- Project snapshots: Project-specific (`~/.config/vm/snapshots/<project>/`)
- Automatic detection based on `@` prefix

### 3. Export/Import Format
- Single `.snapshot.tar.gz` file
- Contains manifest.json with metadata
- Platform-agnostic (includes architecture info)
- Verifiable (checksums included)

## Migration from Current System

**No breaking changes!** Existing workflows continue to work:

```bash
# Existing project snapshot workflow (unchanged)
vm create
vm snapshot create my-state
vm snapshot restore my-state

# NEW: Base image workflow
vm create --snapshot @vibe-base
vm snapshot export @vibe-base
```

## Alternative: Simpler Flag Names

If `--snapshot` is too generic, consider:

```bash
vm create --save-as @vibe-base       # More explicit
vm create --base @vibe-base          # Shorter
vm create --export @vibe-base        # Clearest intent
```

## Questions to Answer

1. Should `vm create --snapshot @name` automatically export to file?
2. Should we auto-detect Dockerfile.vibe in current directory?
3. What happens if you try to restore a base snapshot to a project location?
4. Should base snapshots be immutable (can't overwrite)?
5. Should we support `vm base` as alias for `vm snapshot` with base snapshots?

## My Final Recommendation

**Keep it simple:**

```bash
# Create base image from Dockerfile
vm create --save-base @vibe-base

# Export to share
vm snapshot export @vibe-base

# Import and use
vm snapshot import vibe-base.snapshot.tar.gz
echo "vm.box: @vibe-base" >> vm.yaml
vm create
```

Three simple concepts:
1. `--save-base` = Create reusable base image
2. `@prefix` = Global base snapshot
3. No prefix = Project snapshot

Clear, minimal, powerful. ✨
