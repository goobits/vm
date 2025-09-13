# vm-links: Package Link Detection Replacement

## Problem
- `link-detector.sh` + `pip-detector.sh` (534 lines) are slow
- Sequential filesystem traversal
- Multiple subprocess spawns for each package manager
- No result caching

## Solution
Replace with Rust binary that:
- Parallel directory scanning with rayon
- Direct parsing of package metadata (no subprocess)
- Unified interface for npm/pip/cargo
- Optional result caching

## Performance
- Current: 500ms+ with multiple packages
- Target: 50ms (10x faster, up to 50x with many packages)

## CLI Interface
**EXACTLY these 2 commands, no more:**
```bash
vm-links detect <pkg_manager> <package1> [package2...]  # Output: package:path pairs
vm-links mounts <pkg_manager> <package1> [package2...]  # Output: Docker mount strings
```

**Package managers:** `npm`, `pip`, `cargo` only
**Output format:** Match current shell exactly (for drop-in replacement)
**No auto-detection** - must specify package manager explicitly

## Implementation
- `src/npm.rs` - NPM symlink detection
- `src/pip.rs` - Pip/pipx environment scanning
- `src/cargo.rs` - Cargo install parsing
- `src/main.rs` - Unified CLI
- Uses rayon for parallel scanning

## Migration
1. Build vm-links alongside shell versions
2. Compare outputs to ensure compatibility
3. Replace shell detection once validated