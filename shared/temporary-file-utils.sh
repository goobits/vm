#!/bin/bash
# Centralized temporary file management utilities
# Provides secure temporary file creation and cleanup with proper signal handling

# File-based tracking for temp files (works across process boundaries)
TEMP_FILES_REGISTRY="${TMPDIR:-/tmp}/.vm-temp-files-$$"

# Cleanup mutex to prevent signal handler race conditions
CLEANUP_MUTEX="${TMPDIR:-/tmp}/.vm-cleanup-mutex-$$"

# Initialize the registry file
init_temp_registry() {
    if [[ ! -f "$TEMP_FILES_REGISTRY" ]]; then
        touch "$TEMP_FILES_REGISTRY"
        chmod 600 "$TEMP_FILES_REGISTRY"
    fi
}

# Add a temp file to the registry with validation
register_temp_file() {
    local temp_file="$1"

    # Validate input
    if [[ -z "$temp_file" ]]; then
        echo "❌ Error: Cannot register empty temp file path" >&2
        return 1
    fi

    # Initialize registry
    if ! init_temp_registry; then
        echo "❌ Error: Failed to initialize temp file registry" >&2
        return 1
    fi

    # Check if already registered (avoid duplicates)
    if [[ -f "$TEMP_FILES_REGISTRY" ]] && grep -Fxq "$temp_file" "$TEMP_FILES_REGISTRY" 2>/dev/null; then
        if [[ "${VM_DEBUG:-}" = "true" ]]; then
            echo "⚠️ Warning: Temp file already registered: $temp_file" >&2
        fi
        return 0
    fi

    # Register the file
    if ! echo "$temp_file" >> "$TEMP_FILES_REGISTRY"; then
        echo "❌ Error: Failed to register temp file: $temp_file" >&2
        return 1
    fi

    # Log the operation for debugging and security auditing
    if [[ "${VM_DEBUG:-}" = "true" ]] || [[ "${TEMP_FILE_LOG:-}" = "true" ]]; then
        echo "🗃️  Registered temp file: $temp_file" >&2
    fi

    return 0
}

# Remove a temp file from the registry
unregister_temp_file() {
    local temp_file="$1"
    if [[ -f "$TEMP_FILES_REGISTRY" ]]; then
        grep -v "^$temp_file$" "$TEMP_FILES_REGISTRY" > "${TEMP_FILES_REGISTRY}.tmp" || true
        mv "${TEMP_FILES_REGISTRY}.tmp" "$TEMP_FILES_REGISTRY"

        # Log the operation for debugging and security auditing
        if [[ "${VM_DEBUG:-}" = "true" ]] || [[ "${TEMP_FILE_LOG:-}" = "true" ]]; then
            echo "🗑️  Unregistered temp file: $temp_file" >&2
        fi
    fi
}

# Acquire cleanup mutex to prevent race conditions
acquire_cleanup_mutex() {
    local max_attempts=10
    local attempt=1
    local our_pid=$$

    while [[ $attempt -le $max_attempts ]]; do
        # Try to acquire mutex atomically
        if (set -C; echo "$our_pid" > "$CLEANUP_MUTEX") 2>/dev/null; then
            return 0  # Successfully acquired mutex
        fi

        # Check if existing mutex holder is still alive
        if [[ -f "$CLEANUP_MUTEX" ]]; then
            local mutex_pid
            mutex_pid=$(cat "$CLEANUP_MUTEX" 2>/dev/null)

            # If PID is empty or process doesn't exist, remove stale mutex
            if [[ -z "$mutex_pid" ]] || ! kill -0 "$mutex_pid" 2>/dev/null; then
                rm -f "$CLEANUP_MUTEX" 2>/dev/null || true
                continue  # Try again
            fi
        fi

        # Wait a short time before retrying
        sleep 0.1
        ((attempt++))
    done

    return 1  # Failed to acquire mutex
}

# Release cleanup mutex
release_cleanup_mutex() {
    local our_pid=$$

    # Only remove if we own the mutex
    if [[ -f "$CLEANUP_MUTEX" ]]; then
        local mutex_pid
        mutex_pid=$(cat "$CLEANUP_MUTEX" 2>/dev/null)
        if [[ "$mutex_pid" == "$our_pid" ]]; then
            rm -f "$CLEANUP_MUTEX" 2>/dev/null || true
        fi
    fi
}

# Atomic cleanup function that removes all tracked temporary files with validation
cleanup_temp_files() {
    local exit_code=${1:-0}
    local signal_name=${2:-""}
    local cleanup_errors=0
    local cleanup_failures=()

    # Acquire mutex to prevent concurrent cleanup (race condition protection)
    if ! acquire_cleanup_mutex; then
        if [[ "${VM_DEBUG:-}" = "true" ]]; then
            echo "⚠️ Warning: Could not acquire cleanup mutex, another cleanup may be in progress" >&2
        fi
        # Still proceed but note the potential race
    fi

    # Ensure mutex is released on function exit
    trap 'release_cleanup_mutex' RETURN

    if [[ -f "$TEMP_FILES_REGISTRY" ]] && [[ -s "$TEMP_FILES_REGISTRY" ]]; then
        local file_count
        file_count=$(wc -l < "$TEMP_FILES_REGISTRY" 2>/dev/null || echo "0")

        if [[ $file_count -gt 0 ]]; then
            if [[ -n "$signal_name" ]]; then
                echo "🧹 Cleaning up $file_count temporary file(s) due to $signal_name signal..."
            else
                # Silent cleanup on normal exit unless VM_DEBUG is set
                if [[ "${VM_DEBUG:-}" = "true" ]]; then
                    echo "🧹 Cleaning up $file_count temporary file(s)..."
                fi
            fi

            # Validate temp file paths before deletion
            while IFS= read -r temp_file; do
                if [[ -n "$temp_file" ]]; then
                    # Security validation: ensure temp file is in expected location
                    if [[ "$temp_file" != /tmp/* ]] && [[ "$temp_file" != "${TMPDIR:-/tmp}"/* ]]; then
                        echo "⚠️ Warning: Skipping cleanup of file outside temp directory: $temp_file" >&2
                        cleanup_failures+=("$temp_file (outside temp dir)")
                        ((cleanup_errors++))
                        continue
                    fi

                    # Attempt cleanup with error tracking
                    if [[ -f "$temp_file" ]]; then
                        if ! rm -f "$temp_file" 2>/dev/null; then
                            cleanup_failures+=("$temp_file (file removal failed)")
                            ((cleanup_errors++))
                        fi
                    elif [[ -d "$temp_file" ]]; then
                        if ! rm -rf "$temp_file" 2>/dev/null; then
                            cleanup_failures+=("$temp_file (directory removal failed)")
                            ((cleanup_errors++))
                        fi
                    fi
                fi
            done < "$TEMP_FILES_REGISTRY"

            # Report cleanup status
            if [[ $cleanup_errors -gt 0 ]]; then
                echo "⚠️ Warning: Failed to clean up $cleanup_errors temporary files:" >&2
                for failure in "${cleanup_failures[@]}"; do
                    echo "  - $failure" >&2
                done

                # Log cleanup failures for debugging
                if command -v logger >/dev/null 2>&1; then
                    logger -t vm-temp-cleanup "Failed to cleanup temp files: ${cleanup_failures[*]}"
                fi
            elif [[ "${VM_DEBUG:-}" = "true" ]]; then
                echo "✅ All temporary files cleaned up successfully"
            fi

            # Clear the registry (attempt even if some cleanups failed)
            if ! rm -f "$TEMP_FILES_REGISTRY" 2>/dev/null; then
                echo "⚠️ Warning: Failed to remove temp files registry: $TEMP_FILES_REGISTRY" >&2
            fi
        fi
    fi

    # Validate cleanup effectiveness if debugging is enabled
    if [[ "${VM_DEBUG:-}" = "true" ]] && [[ -z "$signal_name" ]]; then
        # Brief validation check (don't fail on validation errors during cleanup)
        validate_temp_file_cleanup >/dev/null 2>&1 || true
    fi

    # If this was called due to a signal, exit with appropriate code
    if [[ -n "$signal_name" ]]; then
        if [[ "$signal_name" == "INT" ]]; then
            exit 130  # Standard exit code for SIGINT
        elif [[ "$signal_name" == "TERM" ]]; then
            exit 143  # Standard exit code for SIGTERM
        else
            exit 1
        fi
    fi

    # Return error count for validation
    return $cleanup_errors
}

# Set up signal handlers for proper cleanup
setup_temp_file_handlers() {
    # Only set up handlers if they haven't been set already
    if [[ -z "${TEMP_FILE_HANDLERS_SET:-}" ]]; then
        # Clean up on normal exit
        trap 'cleanup_temp_files $?' EXIT

        # Handle interruption signals with proper exit codes
        trap 'cleanup_temp_files $? "INT"' INT
        trap 'cleanup_temp_files $? "TERM"' TERM

        # Handle other signals that might leave temp files
        trap 'cleanup_temp_files $? "HUP"' HUP
        trap 'cleanup_temp_files $? "QUIT"' QUIT

        # Mark that handlers have been set
        export TEMP_FILE_HANDLERS_SET=1
    fi
}

# Create a secure temporary file and track it for cleanup with validation
# Usage: create_temp_file [template] [directory]
# Returns: path to the created temporary file
create_temp_file() {
    local template="${1:-vm-temp.XXXXXX}"
    local temp_dir="${2:-/tmp}"

    # Validate input parameters
    if [[ -z "$template" ]]; then
        echo "❌ Error: Template cannot be empty" >&2
        return 1
    fi

    # Ensure template has at least 3 X's for security
    if [[ ! "$template" =~ XXX ]]; then
        echo "❌ Error: Temporary file template must contain at least 3 X's" >&2
        return 1
    fi

    # Validate temp directory
    if [[ ! -d "$temp_dir" ]]; then
        echo "❌ Error: Temporary directory does not exist: $temp_dir" >&2
        return 1
    fi

    if [[ ! -w "$temp_dir" ]]; then
        echo "❌ Error: No write permission to temporary directory: $temp_dir" >&2
        return 1
    fi

    # Create the temporary file
    local temp_file
    if ! temp_file=$(mktemp "$temp_dir/$template" 2>/dev/null); then
        echo "❌ Error: Failed to create temporary file with template: $temp_dir/$template" >&2
        echo "💡 Check disk space and permissions in $temp_dir" >&2
        return 1
    fi

    # Validate the created file path (security check)
    if [[ "$temp_file" != "$temp_dir"/* ]]; then
        echo "❌ Error: Created temp file is outside expected directory: $temp_file" >&2
        rm -f "$temp_file" 2>/dev/null || true
        return 1
    fi

    # Track it for cleanup
    if ! register_temp_file "$temp_file"; then
        echo "❌ Error: Failed to register temp file for cleanup" >&2
        rm -f "$temp_file" 2>/dev/null || true
        return 1
    fi

    # Set secure permissions (readable/writable only by owner)
    if ! chmod 600 "$temp_file" 2>/dev/null; then
        echo "⚠️ Warning: Failed to set secure permissions on temp file: $temp_file" >&2
    fi

    # Validate file was created successfully
    if [[ ! -f "$temp_file" ]]; then
        echo "❌ Error: Temp file validation failed - file does not exist: $temp_file" >&2
        unregister_temp_file "$temp_file"
        return 1
    fi

    # Log creation for security auditing
    if [[ "${TEMP_FILE_LOG:-}" = "true" ]]; then
        echo "📄 Created temp file: $temp_file (template: $template)" >&2
    fi

    echo "$temp_file"
}

# Create a secure temporary directory and track it for cleanup
# Usage: create_temp_dir [template] [directory]
# Returns: path to the created temporary directory
create_temp_dir() {
    local template="${1:-vm-temp-dir.XXXXXX}"
    local temp_dir="${2:-/tmp}"

    # Ensure template has at least 3 X's for security
    if [[ ! "$template" =~ XXX ]]; then
        echo "❌ Error: Temporary directory template must contain at least 3 X's" >&2
        return 1
    fi

    # Create the temporary directory
    local temp_directory
    if ! temp_directory=$(mktemp -d "$temp_dir/$template" 2>/dev/null); then
        echo "❌ Error: Failed to create temporary directory with template: $temp_dir/$template" >&2
        return 1
    fi

    # Track it for cleanup
    register_temp_file "$temp_directory"

    # Set secure permissions (readable/writable/executable only by owner)
    chmod 700 "$temp_directory" 2>/dev/null || true

    # Log creation for security auditing
    if [[ "${TEMP_FILE_LOG:-}" = "true" ]]; then
        echo "📁 Created temp directory: $temp_directory (template: $template)" >&2
    fi

    echo "$temp_directory"
}


# Function to manually remove a temporary file from tracking
# Usage: untrack_temp_file /path/to/temp/file
untrack_temp_file() {
    local file_to_remove="$1"
    unregister_temp_file "$file_to_remove"
}

# Function to get count of tracked temporary files (for debugging)
get_temp_file_count() {
    if [[ -f "$TEMP_FILES_REGISTRY" ]]; then
        wc -l < "$TEMP_FILES_REGISTRY" 2>/dev/null || echo "0"
    else
        echo "0"
    fi
}

# Function to validate temp file cleanup effectiveness
# Returns 0 if cleanup is working properly, 1 if issues detected
validate_temp_file_cleanup() {
    local validation_errors=0
    local validation_warnings=()

    # Check if registry file exists but is stale
    if [[ -f "$TEMP_FILES_REGISTRY" ]]; then
        local registry_age
        if command -v stat >/dev/null 2>&1; then
            if [[ "$(uname)" == "Darwin" ]]; then
                # macOS stat format
                registry_age=$(stat -f "%m" "$TEMP_FILES_REGISTRY" 2>/dev/null || echo "0")
            else
                # Linux stat format
                registry_age=$(stat -c "%Y" "$TEMP_FILES_REGISTRY" 2>/dev/null || echo "0")
            fi

            local current_time
            current_time=$(date +%s)
            local age_seconds=$((current_time - registry_age))

            # Warn if registry is older than 1 hour (3600 seconds)
            if [[ $age_seconds -gt 3600 ]]; then
                validation_warnings+=("Registry file is stale (${age_seconds}s old)")
            fi
        fi

        # Check for orphaned temp files listed in registry
        local orphaned_count=0
        while IFS= read -r temp_file; do
            if [[ -n "$temp_file" ]] && [[ -e "$temp_file" ]]; then
                ((orphaned_count++))
                validation_warnings+=("Orphaned temp file exists: $temp_file")
            fi
        done < "$TEMP_FILES_REGISTRY" 2>/dev/null

        if [[ $orphaned_count -gt 0 ]]; then
            echo "⚠️ Warning: Found $orphaned_count orphaned temporary files" >&2
            ((validation_errors++))
        fi
    fi

    # Check for temp files in system temp directory that match our patterns
    local stray_files_count=0
    local temp_dirs=("${TMPDIR:-/tmp}" "/tmp")

    for temp_dir in "${temp_dirs[@]}"; do
        if [[ -d "$temp_dir" ]] && [[ -r "$temp_dir" ]]; then
            # Look for files matching our patterns that are older than 1 hour
            while IFS= read -r -d '' stray_file; do
                if [[ -f "$stray_file" ]]; then
                    ((stray_files_count++))
                    if [[ $stray_files_count -le 5 ]]; then  # Limit output
                        validation_warnings+=("Stray temp file: $(basename "$stray_file")")
                    fi
                fi
            done < <(find "$temp_dir" -maxdepth 1 -name "vm-*.XXXXXX*" -o -name "vm-temp.*" -o -name ".vm-temp-files-*" -mmin +60 -print0 2>/dev/null)
        fi
    done

    if [[ $stray_files_count -gt 0 ]]; then
        echo "⚠️ Warning: Found $stray_files_count stray temporary files" >&2
        if [[ $stray_files_count -gt 5 ]]; then
            echo "   (showing first 5, $((stray_files_count - 5)) more not shown)" >&2
        fi
        ((validation_errors++))
    fi

    # Report all warnings
    if [[ ${#validation_warnings[@]} -gt 0 ]]; then
        echo "🔍 Temp file cleanup validation issues:" >&2
        for warning in "${validation_warnings[@]}"; do
            echo "   - $warning" >&2
        done
        echo "💡 Consider running manual cleanup or checking for stuck processes" >&2
    elif [[ "${VM_DEBUG:-}" = "true" ]]; then
        echo "✅ Temp file cleanup validation passed" >&2
    fi

    return $validation_errors
}

# Function to list all tracked temporary files (for debugging)
list_temp_files() {
    if [[ ! -f "$TEMP_FILES_REGISTRY" ]] || [[ ! -s "$TEMP_FILES_REGISTRY" ]]; then
        echo "No temporary files currently tracked"
    else
        echo "Tracked temporary files:"
        local file_count=0
        while IFS= read -r temp_file; do
            ((file_count++))
            local status=""
            if [[ -e "$temp_file" ]]; then
                if [[ -f "$temp_file" ]]; then
                    local size
                    size=$(wc -c < "$temp_file" 2>/dev/null || echo "unknown")
                    status=" (file, ${size} bytes)"
                elif [[ -d "$temp_file" ]]; then
                    status=" (directory)"
                else
                    status=" (other)"
                fi
            else
                status=" (missing)"
            fi
            echo "  $temp_file$status"
        done < "$TEMP_FILES_REGISTRY"
        echo "Total tracked files: $file_count"
    fi
}

# Function to force cleanup of all temp files (emergency cleanup)
force_cleanup_temp_files() {
    echo "🚨 Performing emergency temp file cleanup..." >&2

    # First try normal cleanup
    cleanup_temp_files 0

    # Then clean up any remaining VM temp files in system temp directories
    local temp_dirs=("${TMPDIR:-/tmp}" "/tmp")
    local cleaned_count=0

    for temp_dir in "${temp_dirs[@]}"; do
        if [[ -d "$temp_dir" ]] && [[ -w "$temp_dir" ]]; then
            # Clean up VM-related temp files
            while IFS= read -r -d '' stray_file; do
                if [[ -f "$stray_file" ]] && rm -f "$stray_file" 2>/dev/null; then
                    ((cleaned_count++))
                    if [[ "${VM_DEBUG:-}" = "true" ]]; then
                        echo "   Removed: $(basename "$stray_file")" >&2
                    fi
                fi
            done < <(find "$temp_dir" -maxdepth 1 \( -name "vm-*.XXXXXX*" -o -name "vm-temp.*" -o -name ".vm-temp-files-*" \) -print0 2>/dev/null)
        fi
    done

    if [[ $cleaned_count -gt 0 ]]; then
        echo "🧹 Emergency cleanup removed $cleaned_count additional temp files" >&2
    else
        echo "✅ No additional temp files needed cleanup" >&2
    fi

    return 0
}

# Simple temp file cleanup with trap handlers (for backward compatibility)
# This provides a simpler interface for setting up cleanup of a single temporary file
# Usage: setup_temp_file_cleanup /path/to/temp/file
setup_temp_file_cleanup() {
    local temp_file="$1"

    # Validate input
    if [[ -z "$temp_file" ]]; then
        echo "❌ Error: No temp file path provided for cleanup setup" >&2
        return 1
    fi

    # Set up trap for this specific file
    # Note: This will override any existing EXIT trap, so use with caution
    trap "rm -f \"$temp_file\" 2>/dev/null" EXIT INT TERM

    # Log setup if debugging is enabled
    if [[ "${VM_DEBUG:-}" = "true" ]] || [[ "${TEMP_FILE_LOG:-}" = "true" ]]; then
        echo "🗑️  Set up cleanup trap for: $temp_file" >&2
    fi

    return 0
}