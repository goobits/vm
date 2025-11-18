# Bug Report: `vm snapshot create --from-dockerfile` Missing Build Context

## Environment
- **VM Version**: 4.4.1
- **Platform**: macOS 15.6.1 (arm64)
- **Docker**: 28.4.0

## Issue Summary
The `vm snapshot create --from-dockerfile` command fails with "please specify build context" error, even though the help text doesn't indicate that a build context parameter is needed or available.

## Expected Behavior
When running:
```bash
vm snapshot create vibe-box --from-dockerfile Dockerfile.vibe --description "Vibe development environment"
```

The command should either:
1. **Option A**: Automatically use the current directory (`.`) as the build context (most intuitive)
2. **Option B**: Accept a `--build-context` or `--context` flag to specify the build context
3. **Option C**: Show in `--help` that this feature requires additional configuration

## Actual Behavior
The command fails with:
```
Building snapshot 'vibe-box' from Dockerfile...
Description: Vibe development environment with Node.js, Python, Rust, Playwright, and AI CLI tools
Build context:
Dockerfile: Dockerfile.vibe
ERROR: failed to build: please specify build context (e.g. "." for the current directory)
Error: Failed to build Docker image from Dockerfile.vibe
```

## Help Text Output
```bash
$ vm snapshot create --help
Create a snapshot of the current VM state

Usage: vm snapshot create [OPTIONS] <NAME>

Arguments:
  <NAME>  Snapshot name

Options:
      --description <DESCRIPTION>  Optional description
      --quiesce                    Stop services before snapshotting for consistency
      --project <PROJECT>          Project name (auto-detected if omitted)
      --from-dockerfile <PATH>     Build snapshot directly from a Dockerfile
      --build-arg <KEY=VALUE>      Build arguments for Dockerfile (repeatable: --build-arg KEY=VALUE)
  -c, --config <CONFIG>            Path to a custom VM configuration file
      --dry-run                    Show what would be executed without running
  -h, --help                       Print help
```

**Note**: No `--build-context` or `--context` option is listed.

## Reproduction Steps
1. Create a Dockerfile (e.g., `Dockerfile.vibe`)
2. Run: `vm snapshot create vibe-box --from-dockerfile Dockerfile.vibe --description "Test"`
3. Observe the error about missing build context

## Suggested Fix
Add a `--build-context <PATH>` option that defaults to `.` (current directory):

```rust
// In the snapshot create command options
#[arg(long, default_value = ".")]
build_context: Option<PathBuf>,
```

This would allow:
```bash
# Uses current directory by default
vm snapshot create vibe-box --from-dockerfile Dockerfile.vibe

# Or specify different context
vm snapshot create vibe-box --from-dockerfile Dockerfile.vibe --build-context /path/to/context
```

## Workaround
Currently, users must:
1. Build the Docker image manually: `docker build -f Dockerfile.vibe -t vibe-box:latest .`
2. Then import it (though it's unclear how to convert a local image to a snapshot)

## Additional Context
This issue was encountered while trying to create a base development environment snapshot from a Dockerfile as part of the "golden image" workflow mentioned in the VM tool documentation.

## Related Files
- `/Users/miko/projects/vm/Dockerfile.vibe` - The Dockerfile attempting to be used
- Working directory: `/Users/miko/projects/vm`

## Impact
This blocks users from creating snapshots directly from Dockerfiles, which is a documented feature that should streamline the snapshot creation workflow.
