## Problem

Users want to build custom base images (like Dockerfile.vibe) once and reuse them across:
- Multiple projects on the same machine
- Multiple machines in a team
- CI/CD pipelines
- Air-gapped environments

Current workflow requires either:
1. Rebuilding Dockerfile on every machine (slow, 10-15 min)
2. Pushing to Docker registry (requires auth, network)
3. Manual docker export/import (doesn't integrate with vm tool)

## Goals

Enable portable, shareable base images that work seamlessly with the vm tool:

1. **Build once** from Dockerfile → create reusable snapshot
2. **Export** snapshot as single portable file
3. **Import** snapshot on any machine running vm tool
4. **Reference** imported snapshot with `vm.box: @snapshot-name`
5. **Share** snapshots via file transfer (Dropbox, S3, USB, etc.)

## User Experience

### Machine A: Create and Export

```bash
# Step 1: Create initial vm.yaml for building base image
cat > vm-base.yaml <<EOF
version: 1.2.1
provider: docker
vm:
  box: ./Dockerfile.vibe
project:
  name: vibe-base-builder
EOF

# Step 2: Build the base image
vm create --config vm-base.yaml

# Step 3: Create snapshot from running container
vm snapshot create vibe-base \
  --description "Base vibe image: Node LTS, Python 3.13, Rust, Playwright, AI CLIs" \
  --quiesce

# Step 4: Export as portable file
vm snapshot export vibe-base --output ~/Desktop/vibe-base.snapshot.tar.gz

# ✓ Result: vibe-base.snapshot.tar.gz (2-3 GB)
```

### Machine B: Import and Use

```bash
# Step 1: Import the snapshot
vm snapshot import ~/Downloads/vibe-base.snapshot.tar.gz

# Step 2: Verify it's available
vm snapshot list
# Output:
# NAME        CREATED              SIZE    DESCRIPTION
# vibe-base   2025-10-30 12:34    2.1 GB  Base vibe image: Node LTS...

# Step 3: Use in any project
cat > vm.yaml <<EOF
version: 1.2.1
provider: docker
vm:
  box: @vibe-base  # ← References imported snapshot!
  memory: 8gb
project:
  name: my-project
EOF

# Step 4: Instant creation (no rebuild!)
vm create
# ✓ Starts in 5-10 seconds instead of 10-15 minutes
```

## Architecture

### Snapshot Format

Export creates a tarball containing:

```
vibe-base.snapshot.tar.gz
├── manifest.json              # Metadata + checksum
├── images/
│   └── base.tar              # Docker image (docker save output)
├── metadata/
│   ├── versions.json         # Node, Python, Rust versions
│   ├── packages.json         # Installed packages list
│   └── build-info.json       # Build date, platform, etc.
└── docs/
    └── README.md             # Human-readable description
```

### manifest.json

```json
{
  "version": "1.0",
  "snapshot_name": "vibe-base",
  "created_at": "2025-10-30T19:34:56Z",
  "description": "Base vibe image: Node LTS, Python 3.13, Rust, Playwright, AI CLIs",
  "image_digest": "sha256:abc123...",
  "platform": "linux/amd64",
  "size_bytes": 2147483648,
  "languages": {
    "node": "22.11.0",
    "python": "3.13.0",
    "rust": "1.82.0"
  },
  "tools": [
    "@anthropic-ai/claude-code",
    "@google/gemini-cli",
    "playwright",
    "cargo-watch"
  ]
}
```

### CLI Commands

```bash
# Export snapshot
vm snapshot export <name> [OPTIONS]
  --output <FILE>          # Output file path (default: ./<name>.snapshot.tar.gz)
  --compress <LEVEL>       # Compression level 1-9 (default: 6)
  --include-volumes        # Include volumes (makes it project-specific)
  --project <PROJECT>      # Project name (auto-detected if omitted)

# Import snapshot
vm snapshot import <FILE> [OPTIONS]
  --name <NAME>            # Override snapshot name
  --verify                 # Verify checksum before import
  --force                  # Overwrite existing snapshot

# List available snapshots (enhanced)
vm snapshot list [OPTIONS]
  --format <FORMAT>        # table (default), json, yaml
  --show-digest            # Show image digests
  --filter <FILTER>        # Filter by name pattern
```

### BoxSpec Integration

Already supported in code! From box_config_tests.rs:

```rust
// Snapshot reference with @ prefix
let spec = BoxSpec::String("@vibe-base".to_string());
// → BoxConfig::Snapshot("vibe-base")
```

When user specifies `vm.box: @vibe-base`:

1. VM tool checks `~/.config/vm/snapshots/global/vibe-base/`
2. Loads image from snapshot
3. Uses as base instead of pulling/building
4. Instant startup!

## Implementation Plan

### Phase 1: Export Command (2-3 days)

**File:** `rust/vm/src/commands/snapshot/export.rs`

```rust
pub async fn handle_export(
    name: &str,
    output_path: Option<&str>,
    compress_level: u8,
    include_volumes: bool,
    project: Option<&str>,
) -> VmResult<()> {
    // 1. Validate snapshot exists
    // 2. Create temp directory for tarball contents
    // 3. Save docker image: docker save <image> > images/base.tar
    // 4. Generate manifest.json with metadata
    // 5. Optionally include volumes
    // 6. Create compressed tarball
    // 7. Verify integrity
    // 8. Display export stats
}
```

### Phase 2: Import Command (2-3 days)

**File:** `rust/vm/src/commands/snapshot/import.rs`

```rust
pub async fn handle_import(
    file_path: &str,
    name_override: Option<&str>,
    verify: bool,
    force: bool,
) -> VmResult<()> {
    // 1. Verify tarball integrity (checksums)
    // 2. Extract to temp directory
    // 3. Load manifest.json
    // 4. Check for conflicts (existing snapshot)
    // 5. Load docker image: docker load < images/base.tar
    // 6. Copy to ~/.config/vm/snapshots/global/<name>/
    // 7. Register in snapshot registry
    // 8. Display import stats + usage instructions
}
```

### Phase 3: BoxSpec @snapshot Integration (1 day)

**File:** `rust/vm-provider/src/docker/build.rs`

Already has `BoxConfig::Snapshot` handling at line 129-134, but returns error.
Update to:

```rust
BoxConfig::Snapshot(name) => {
    let snapshot_manager = SnapshotManager::new()?;
    let snapshot_dir = snapshot_manager.get_snapshot_dir(Some("global"), name);

    if !snapshot_dir.exists() {
        return Err(VmError::NotFound(format!(
            "Snapshot '@{}' not found. Import it first with:\n  vm snapshot import <file>",
            name
        )));
    }

    // Load image from snapshot
    let image_path = snapshot_dir.join("images/base.tar");
    DockerOps::load_image(&image_path)?;

    // Get image tag from metadata
    let metadata = SnapshotMetadata::load(snapshot_dir.join("metadata.json"))?;
    metadata.services[0].image_tag.clone()
}
```

### Phase 4: Enhanced List Command (1 day)

Update `vm snapshot list` to show:
- Snapshot type (project-specific vs. global/importable)
- Platform (linux/amd64, etc.)
- Languages/tools installed
- Source (built locally vs. imported)

### Phase 5: Documentation & Examples (1 day)

- Update docs/user-guide/configuration.md
- Add examples/base-images/README.md
- Document export/import workflow
- Add CI/CD integration example

## Non-Goals

- **Multi-architecture support**: Initial version exports for current platform only
- **Incremental exports**: Full export every time (can optimize later)
- **Snapshot registry**: No central repo (users share via file transfer)
- **Automatic updates**: Snapshots are immutable once exported

## Edge Cases & Considerations

### Name Conflicts

```bash
vm snapshot import vibe-base.tar.gz  # Already exists
# Error: Snapshot 'vibe-base' already exists
# Use --force to overwrite or --name to import with different name
```

### Platform Mismatch

```bash
# Export on linux/amd64
vm snapshot export vibe-base --output vibe.tar.gz

# Import on linux/arm64
vm snapshot import vibe.tar.gz
# Warning: Snapshot built for linux/amd64, current platform is linux/arm64
# This may not work correctly. Continue? [y/N]
```

### Large Files

- Progress bars for export/import (can take minutes)
- Parallel compression for faster export
- Resume support for interrupted transfers (future)

### Security

- Verify checksums on import (prevent corruption)
- Optional GPG signing for trusted snapshots (future)
- Scan for secrets before export (warn if API keys detected)

## Success Criteria

✅ User can build Dockerfile.vibe once and export snapshot
✅ Exported snapshot is single portable file (2-3 GB)
✅ Import on new machine takes < 1 minute
✅ Using `vm.box: @snapshot-name` creates VM in < 10 seconds
✅ No Docker Hub or external registry required
✅ Works with air-gapped machines
✅ Team can share snapshots via Dropbox/S3/USB

## Migration Path

For existing users with custom Dockerfiles:

```bash
# Before: Rebuild on every machine
vm create  # 10-15 minutes

# After: Build once, share snapshot
vm snapshot export my-base --output my-base.tar.gz
# Share my-base.tar.gz with team
# Team imports and uses instantly
```

## Future Enhancements

- `vm snapshot push/pull` - Registry support (Docker Hub, GHCR)
- `vm snapshot diff` - Show differences between snapshots
- `vm snapshot merge` - Combine multiple snapshots
- `vm snapshot layers` - Show layer breakdown and optimize size
- Snapshot marketplace/community registry

## Questions for Discussion

1. Should we support incremental exports (only changed layers)?
2. Default snapshot location: global vs. project-specific?
3. Should `vm create --from-dockerfile` auto-create snapshot?
4. Naming convention for exported files? (`.snapshot.tar.gz` vs `.vmsnap`)
5. Should we bundle snapshot import into vm CLI binary? (no docker needed to inspect)

## Example: Team Workflow

```bash
# DevOps creates base image
cd devops/base-images/vibe/
vm create --config vm-base.yaml
vm snapshot create vibe-2024-10 --description "Oct 2024: Node 22, Python 3.13"
vm snapshot export vibe-2024-10 --output s3://company-vms/vibe-2024-10.snapshot.tar.gz

# Developers use it
aws s3 cp s3://company-vms/vibe-2024-10.snapshot.tar.gz ~/Downloads/
vm snapshot import ~/Downloads/vibe-2024-10.snapshot.tar.gz

# In any project
echo "vm.box: @vibe-2024-10" >> vm.yaml
vm create  # Instant!
```

## Alternatives Considered

1. **Use Docker registry** - Requires auth, network, doesn't integrate with vm snapshots
2. **Use vagrant boxes** - Only works with Vagrant provider, not Docker
3. **Manual docker save/load** - Doesn't integrate with vm tool, no metadata
4. **Git LFS** - Too large for most repos, complex setup

## Decision

Implement `vm snapshot export/import` as proposed. Provides best UX for sharing base images without external dependencies.
