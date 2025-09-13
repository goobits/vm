# vm-ports: Port Management Replacement

## Problem
- `port-manager.sh` (237 lines) is slow: 200ms per operation
- Uses jq for JSON parsing with shell loops
- O(n²) conflict checking

## Solution
Replace with Rust binary that:
- Lazy-load JSON file into HashMap for fast operations
- Write back to file only when changed
- O(n) conflict detection with in-memory data
- Atomic file operations (write to temp, then rename)
- Type-safe port ranges

## Performance
- Current: 200ms
- Target: 2ms (100x faster)

## CLI Interface
**EXACTLY these 4 commands, no more:**
```bash
vm-ports check <range> [project_name]     # Exit 0 if no conflicts, 1 if conflicts
vm-ports register <range> <project> <path> # Add to registry
vm-ports suggest [size]                    # Output next available range
vm-ports list                              # Show all registered ranges
```

**Output format:** Match current shell exactly (for drop-in replacement)

## Implementation
- `src/registry.rs` - HashMap-based registry
- `src/range.rs` - Port range validation/overlap
- `src/main.rs` - CLI interface
- Uses workspace deps: serde, clap, anyhow

## Migration
1. Build vm-ports alongside shell version
2. Add feature flag in vm.sh to switch implementations
3. Replace shell calls once tested