# Temporary File Security Improvements

## Overview

This document describes the comprehensive improvements made to fix temporary file race conditions and ensure secure temporary file handling across all vm.sh scripts.

## Issues Fixed

### 1. **Race Conditions in vm.sh**
- **Location**: Lines 394-396 and similar patterns throughout vm.sh
- **Problem**: Temporary files created with `mktemp` but cleanup not guaranteed on script interruption
- **Impact**: Could leave sensitive temporary files containing VM configurations on disk

### 2. **Missing Signal Handlers**
- **Problem**: No trap handlers for EXIT/INT/TERM signals
- **Impact**: Script interruption (Ctrl+C) would leave temporary files behind

### 3. **Inconsistent Cleanup**
- **Problem**: Manual cleanup calls that could fail or be interrupted
- **Impact**: Race conditions where cleanup might not execute before script termination

## Solutions Implemented

### 1. Centralized Temporary File Management (`/workspace/shared/temp-file-utils.sh`)

#### Features:
- **Atomic cleanup**: All temporary files are tracked and cleaned up atomically
- **Cross-process tracking**: Uses file-based registry that works across process boundaries
- **Signal handling**: Proper trap handlers for EXIT, INT, TERM, HUP, and QUIT signals
- **Security logging**: Optional logging for security auditing and debugging
- **Secure permissions**: All temporary files created with 600 permissions (owner read/write only)

#### Key Functions:
```bash
# Create secure temporary file with automatic cleanup
temp_file=$(create_temp_file "template.XXXXXX")

# Create secure temporary directory with automatic cleanup  
temp_dir=$(create_temp_dir "template-dir.XXXXXX")

# Set up signal handlers (called once per script)
setup_temp_file_handlers

# Manual cleanup (usually not needed - automatic via traps)
cleanup_temp_files
```

### 2. Updated Scripts

#### `generate-config.sh`:
- Integrated centralized temp file management
- Removed manual cleanup calls that could cause race conditions
- Added proper signal handling
- Created helper function `safe_yq_update()` for atomic YAML operations

#### `vm.sh`:
- Updated all Docker functions: `docker_up`, `docker_destroy`, `docker_reload`, `docker_provision`
- Replaced manual `mktemp` calls with secure `create_temp_file()` calls
- Removed manual `rm` cleanup calls that could be interrupted
- Added centralized trap handling

#### `vm-temp.sh`:
- Fixed race condition in line 359 where manual cleanup could conflict with trap handlers
- Maintained existing comprehensive cleanup logic while improving consistency

### 3. Security Improvements

#### Atomic Operations:
- All temporary file operations are now atomic
- No partial cleanup states possible
- Signal interruption handled gracefully

#### Permission Security:
- All temporary files created with 600 permissions
- Temporary directories created with 700 permissions
- No world-readable temporary files

#### Process Isolation:
- Temporary file tracking works across process boundaries
- Each process has its own registry file
- Registry files are automatically cleaned up

## Testing

### Test Suite (`test-temp-file-cleanup.sh`)

Comprehensive test suite that verifies:

1. **Normal execution**: No temporary files left after successful completion
2. **Interrupted execution**: Proper cleanup after timeout/signal interruption  
3. **Signal handling**: SIGTERM, SIGINT handled correctly
4. **Cross-process cleanup**: Temporary files cleaned up across process boundaries
5. **Multiple scripts**: generate-config.sh, vm.sh integration testing

### Test Results:
- ✅ All temporary files properly cleaned up after normal execution
- ✅ Signal interruption (SIGTERM) triggers proper cleanup
- ✅ No race conditions detected
- ✅ Cross-process tracking works correctly

## Usage

### For Developers:

#### Basic Usage:
```bash
#!/bin/bash
source "$SCRIPT_DIR/shared/temp-file-utils.sh"
setup_temp_file_handlers

# Create temporary files - they'll be cleaned up automatically
temp_file=$(create_temp_file "my-script.XXXXXX")
echo "data" > "$temp_file"

# Script exits - cleanup happens automatically via trap
```

#### With Logging:
```bash
# Enable logging for debugging/auditing
export TEMP_FILE_LOG=true
# Or use VM_DEBUG=true for general debugging

# Now all temp file operations will be logged
temp_file=$(create_temp_file "debug.XXXXXX")
```

#### Advanced Usage:
```bash
# Create temporary directory
temp_dir=$(create_temp_dir "workdir.XXXXXX")

# Manual untracking (if you move/rename the file)
untrack_temp_file "$temp_file"

# Check how many temp files are tracked
count=$(get_temp_file_count)
echo "Tracking $count temporary files"

# List all tracked files (debugging)
list_temp_files
```

### Configuration Options:

- `VM_DEBUG=true`: Enable debug output including temp file operations
- `TEMP_FILE_LOG=true`: Enable detailed logging of all temp file operations
- Custom temp directory: `create_temp_file "template.XXXXXX" "/custom/tmp/dir"`

## Security Benefits

1. **No Temporary File Leaks**: Guaranteed cleanup even on script interruption
2. **Secure Permissions**: All temporary files created with restrictive permissions
3. **Audit Trail**: Optional logging for security monitoring
4. **Process Isolation**: Each script instance manages its own temporary files
5. **Signal Safety**: Proper handling of all common termination signals

## Backward Compatibility

- All existing scripts continue to work without modification
- New centralized system is opt-in for new code
- Old manual cleanup methods still work but are now redundant

## Performance Impact

- Minimal overhead: File-based tracking adds negligible performance cost
- Atomic operations: No performance degradation for normal operations
- Signal handling: Standard bash trap mechanisms, no performance impact

## Maintenance

The centralized system makes temporary file management:
- **Consistent**: Same patterns across all scripts
- **Testable**: Comprehensive test suite ensures reliability  
- **Auditable**: Optional logging provides visibility into temp file operations
- **Secure**: Default secure permissions and guaranteed cleanup