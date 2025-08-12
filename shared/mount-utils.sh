#!/bin/bash
# Shared Mount Validation and Processing Utilities
# Extracted from vm.sh for use by both Docker and Vagrant providers
#
# This module provides provider-agnostic mount validation, security checking,
# and mount argument construction utilities.

# Source docker-utils.sh for construct_mount_argument function
MOUNT_UTILS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$MOUNT_UTILS_DIR/docker-utils.sh"

# Lightweight atomic security validation for TOCTOU prevention
# This function performs essential security checks on already-resolved paths
validate_mount_security_atomic() {
    local resolved_path="$1"

    # Validate input
    if [[ -z "$resolved_path" ]]; then
        echo "❌ Error: Empty resolved path provided" >&2
        return 1
    fi

    # 1. Protect system-critical paths (check resolved real path)
    local protected_paths=(
        '/'                 # Root filesystem
        '/root'             # Root user home
        '/boot'             # Boot files
        '/proc'             # Process information
        '/sys'              # System information
        '/dev'              # Device files
        '/var/log'          # System logs
        '/etc'              # System configuration
        '/bin'              # Essential binaries
        '/sbin'             # System binaries
        '/usr/bin'          # User binaries
        '/usr/sbin'         # System administration binaries
        '/lib'              # Essential libraries
        '/lib64'            # 64-bit libraries
        '/usr/lib'          # User libraries
        '/var/lib'          # Variable state information
        '/var/cache'        # Cache files
        '/var/spool'        # Spool files
        '/var/run'          # Runtime files
        '/run'              # Runtime files (modern)
        '/snap'             # Snap packages
        '/media'            # Removable media
        '/mnt'              # Mount points (could be system mounts)
    )

    for protected in "${protected_paths[@]}"; do
        # Check if the real path starts with or equals the protected path
        if [[ "$resolved_path" == "$protected" ]] || [[ "$resolved_path" == "$protected"/* ]]; then
            echo "❌ Error: Cannot mount system-critical path" >&2
            echo "💡 Path '$resolved_path' is within protected system directory '$protected'" >&2
            return 1
        fi
    done

    # 2. Whitelist approach - only allow common development directories
    local allowed_path_prefixes=(
        "/home/"            # User home directories
        "/tmp/"             # Temporary files
        "/var/tmp/"         # Temporary files
        "/workspace/"       # Common workspace
        "/opt/"             # Optional software
        "/srv/"             # Service data
        "/usr/local/"       # User-installed software
        "/data/"            # Common data directory
        "/projects/"        # Common projects directory
    )

    # Special case: allow current working directory and its subdirectories
    local current_dir
    current_dir=$(pwd)
    allowed_path_prefixes+=("$current_dir/")

    # Check if the path is in an allowed directory
    local path_allowed=false
    for allowed_prefix in "${allowed_path_prefixes[@]}"; do
        if [[ "$resolved_path" == "$allowed_prefix"* ]] || [[ "$resolved_path" == "${allowed_prefix%/}" ]]; then
            path_allowed=true
            break
        fi
    done

    if [[ "$path_allowed" == false ]]; then
        echo "❌ Error: Directory path not in allowed locations" >&2
        echo "💡 Only directories under these paths are allowed:" >&2
        printf "   %s\n" "${allowed_path_prefixes[@]}" >&2
        echo "   Current directory: $current_dir" >&2
        return 1
    fi

    # 3. Additional validation for absolute paths
    if [[ "$resolved_path" == "/" ]]; then
        echo "❌ Error: Cannot mount root filesystem" >&2
        return 1
    fi

    return 0
}

# Validate mount directory security (dangerous characters and path traversal)
validate_mount_security() {
    local dir_path="$1"

    # Resolve the real path to handle symlinks and get canonical path
    local real_path
    if ! real_path=$(realpath "$dir_path" 2>/dev/null); then
        echo "❌ Error: Cannot resolve path '$dir_path'" >&2
        return 1
    fi

    # 1. Check for dangerous shell metacharacters using case statement for reliability
    case "$dir_path" in
        *\;* | *\`* | *\$* | *\"* | *\|* | *\&* | *\>* | *\<* | *\(* | *\)* | *\{* | *\}* | *\** | *\?* | *\[* | *\]* | *~* | *@* | *#* | *%*)
            echo "❌ Error: Directory path contains potentially dangerous characters" >&2
            echo "💡 Directory paths cannot contain: ; \` $ \" | & > < ( ) { } * ? [ ] ~ @ # %" >&2
            return 1
            ;;
    esac

    # 2. Check for path traversal attempts (including encoded variants and Unicode)
    local path_patterns=(
        '\.\.'              # Basic ..
        '%2e%2e'            # URL encoded ..
        '%252e%252e'        # Double URL encoded ..
        '\.%2e'             # Mixed encoding
        '%2e\.'             # Mixed encoding
        '\x2e\x2e'          # Hex encoded ..
        '..%2f'             # .. with encoded slash
        '..%5c'             # .. with encoded backslash
        '%2e%2e%2f'         # Full URL encoded ../
        '%2e%2e%5c'         # Full URL encoded ..\
    )

    # First check ASCII patterns
    for pattern in "${path_patterns[@]}"; do
        if [[ "$dir_path" =~ $pattern ]]; then
            echo "❌ Error: Directory path traversal attempt detected" >&2
            echo "💡 Path contains suspicious pattern: $pattern" >&2
            return 1
        fi
    done

    # Unicode normalization security check using Python for comprehensive coverage
    # This handles Unicode-encoded dots and other Unicode normalization attacks
    if command -v python3 >/dev/null 2>&1; then
        local unicode_check_result
        unicode_check_result=$(python3 -c "
import unicodedata
import sys
import re

path = sys.argv[1] if len(sys.argv) > 1 else ''

# Normalize the path using different Unicode normalization forms
normalized_nfc = unicodedata.normalize('NFC', path)
normalized_nfd = unicodedata.normalize('NFD', path)
normalized_nfkc = unicodedata.normalize('NFKC', path)
normalized_nfkd = unicodedata.normalize('NFKD', path)

# Define dangerous Unicode patterns
unicode_patterns = [
    # Unicode-encoded dots (various forms)
    r'\\\\u002e\\\\u002e',          # Unicode-encoded ..
    r'\\\\uff0e\\\\uff0e',          # Fullwidth Unicode dots
    r'\\\\u2024\\\\u2024',          # One-dot leaders
    r'\\\\u2025\\\\u2025',          # Two-dot leaders
    r'\\\\u22ef',                 # Midline horizontal ellipsis
    r'\\\\u2026',                 # Horizontal ellipsis
    # Mixed Unicode/ASCII combinations
    r'\\\\u002e\\.',               # Unicode dot + ASCII dot
    r'\\.\\\\u002e',               # ASCII dot + Unicode dot
    r'\\\\uff0e\\.',               # Fullwidth + ASCII
    r'\\.\\\\uff0e',               # ASCII + Fullwidth
]

# Check all normalized forms
all_forms = [path, normalized_nfc, normalized_nfd, normalized_nfkc, normalized_nfkd]

for form in all_forms:
    # Check for literal Unicode sequences in the string
    for pattern in unicode_patterns:
        if re.search(pattern, form):
            print('UNICODE_ATTACK_DETECTED')
            sys.exit(1)

    # Check if normalization reveals .. patterns
    if '..' in form and form != path:
        print('UNICODE_NORMALIZATION_ATTACK')
        sys.exit(1)

    # Check for encoded Unicode dot sequences that normalize to ..
    if re.search(r'\\.{2,}', form) and '.' not in path:
        print('UNICODE_DOT_ATTACK')
        sys.exit(1)

print('UNICODE_SAFE')
" "$dir_path" 2>/dev/null)

        case "$unicode_check_result" in
            "UNICODE_ATTACK_DETECTED")
                echo "❌ Error: Unicode-encoded path traversal attempt detected" >&2
                echo "💡 Path contains Unicode-encoded dangerous characters" >&2
                return 1
                ;;
            "UNICODE_NORMALIZATION_ATTACK")
                echo "❌ Error: Unicode normalization attack detected" >&2
                echo "💡 Path normalizes to contain path traversal sequences" >&2
                return 1
                ;;
            "UNICODE_DOT_ATTACK")
                echo "❌ Error: Unicode dot sequence attack detected" >&2
                echo "💡 Hidden Unicode characters normalize to path traversal" >&2
                return 1
                ;;
            "UNICODE_SAFE")
                # Path passed Unicode checks, continue
                ;;
            *)
                # Python check failed, fall back to basic validation but warn
                if [[ "${VM_DEBUG:-}" = "true" ]]; then
                    echo "⚠️ Warning: Unicode validation unavailable, using basic checks only" >&2
                fi
                ;;
        esac
    else
        # Python not available, use simpler fallback checks
        if [[ "${VM_DEBUG:-}" = "true" ]]; then
            echo "⚠️ Warning: Python3 not available, Unicode attack detection limited" >&2
        fi

        # Basic Unicode pattern detection using grep (limited but better than nothing)
        if echo "$dir_path" | grep -qE '\\u[0-9a-fA-F]{4}|\\uff[0-9a-fA-F]{2}|\\u202[4-6]|\\u22ef'; then
            echo "❌ Error: Possible Unicode-encoded characters detected" >&2
            echo "💡 Install python3 for comprehensive Unicode attack detection" >&2
            return 1
        fi
    fi

    # 3. Protect system-critical paths (check resolved real path)
    local protected_paths=(
        '/'                 # Root filesystem
        '/root'             # Root user home
        '/boot'             # Boot files
        '/proc'             # Process information
        '/sys'              # System information
        '/dev'              # Device files
        '/var/log'          # System logs
        '/etc'              # System configuration
        '/bin'              # Essential binaries
        '/sbin'             # System binaries
        '/usr/bin'          # User binaries
        '/usr/sbin'         # System administration binaries
        '/lib'              # Essential libraries
        '/lib64'            # 64-bit libraries
        '/usr/lib'          # User libraries
        '/var/lib'          # Variable state information
        '/var/cache'        # Cache files
        '/var/spool'        # Spool files
        '/var/run'          # Runtime files
        '/run'              # Runtime files (modern)
        '/snap'             # Snap packages
        '/media'            # Removable media
        '/mnt'              # Mount points (could be system mounts)
    )

    for protected in "${protected_paths[@]}"; do
        # Check if the real path starts with or equals the protected path
        if [[ "$real_path" == "$protected" ]] || [[ "$real_path" == "$protected"/* ]]; then
            echo "❌ Error: Cannot mount system-critical path" >&2
            echo "💡 Path '$real_path' is within protected system directory '$protected'" >&2
            return 1
        fi
    done

    # 4. Whitelist approach - only allow common development directories
    local allowed_path_prefixes=(
        "/home/"            # User home directories
        "/tmp/"             # Temporary files
        "/var/tmp/"         # Temporary files
        "/workspace/"       # Common workspace
        "/opt/"             # Optional software
        "/srv/"             # Service data
        "/usr/local/"       # User-installed software
        "/data/"            # Common data directory
        "/projects/"        # Common projects directory
    )

    # Special case: allow current working directory and its subdirectories
    local current_dir
    current_dir=$(pwd)
    allowed_path_prefixes+=("$current_dir/")

    # Check if the path is in an allowed directory
    local path_allowed=false
    for allowed_prefix in "${allowed_path_prefixes[@]}"; do
        if [[ "$real_path" == "$allowed_prefix"* ]] || [[ "$real_path" == "${allowed_prefix%/}" ]]; then
            path_allowed=true
            break
        fi
    done

    if [[ "$path_allowed" == false ]]; then
        echo "❌ Error: Directory path not in allowed locations" >&2
        echo "💡 Only directories under these paths are allowed:" >&2
        printf "   %s\n" "${allowed_path_prefixes[@]}" >&2
        echo "   Current directory: $current_dir" >&2
        return 1
    fi

    # 5. Additional validation for absolute paths
    if [[ "$real_path" == "/" ]]; then
        echo "❌ Error: Cannot mount root filesystem" >&2
        return 1
    fi

    # 6. Check for dangerous control characters using length check
    # If the string length changes when we remove dangerous chars, they were present
    local clean_path="${dir_path//[$'\0\n\r']/}"
    if [[ ${#clean_path} -ne ${#dir_path} ]]; then
        echo "❌ Error: Directory path contains dangerous control characters" >&2
        return 1
    fi

    return 0
}

# Detect potential comma-in-directory-name issues by analyzing parsed fragments
detect_comma_in_paths() {
    local -n mounts_array="$1"
    local suspicious_count=0
    local total_count=${#mounts_array[@]}

    # Check if any parsed fragment looks suspicious (very short, no path separators)
    for test_mount in "${mounts_array[@]}"; do
        test_mount=$(echo "$test_mount" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
        # Remove permission suffix for testing
        if [[ "$test_mount" == *:* ]]; then
            test_mount="${test_mount%:*}"
        fi
        # Very short fragments (1-2 chars) without slashes are suspicious
        if [[ -n "$test_mount" ]] && [[ ${#test_mount} -le 2 ]] && [[ "$test_mount" != *"/"* ]] && [[ "$test_mount" != "."* ]]; then
            ((suspicious_count++))
        fi
    done

    # If more than half the fragments are suspicious short names, likely comma issue
    if [[ $total_count -gt 2 ]] && [[ $suspicious_count -gt $((total_count / 2)) ]]; then
        echo "❌ Error: Possible comma-containing directory names detected" >&2
        echo "   Parsed fragments: ${mounts_array[*]}" >&2
        echo "   Directory names containing commas are not supported" >&2
        echo "   Tip: Use symlinks like: ln -s 'dir,with,commas' dir-without-commas" >&2
        return 1
    fi

    return 0
}

# Parse mount permissions and return appropriate mount flags
# This function is provider-agnostic and returns generic permission indicators
parse_mount_permissions() {
    local perm="$1"
    local mount_flags=""

    case "$perm" in
        "ro"|"readonly")
            mount_flags=":ro"
            ;;
        "rw"|"readwrite"|*)
            # Default to read-write (no additional flags)
            mount_flags=""
            ;;
    esac

    echo "$mount_flags"
}


# Process a single mount specification (with or without permissions) with enhanced error handling
process_single_mount() {
    local mount="$1"
    local source=""
    local perm=""

    # Validate input
    if [[ -z "$mount" ]]; then
        echo "❌ Error: Empty mount specification provided" >&2
        return 1
    fi

    # Trim whitespace
    mount=$(echo "$mount" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')

    # Handle mount:permission format (e.g., ./src:rw, ./config:ro)
    if [[ "$mount" == *:* ]]; then
        source="${mount%:*}"
        perm="${mount##*:}"

        # Validate permission format
        if [[ "$perm" != "rw" ]] && [[ "$perm" != "ro" ]] && [[ "$perm" != "readonly" ]] && [[ "$perm" != "readwrite" ]]; then
            echo "❌ Error: Invalid permission '$perm' in mount '$mount'" >&2
            echo "💡 Valid permissions: rw (read-write), ro (read-only)" >&2
            return 1
        fi
    else
        source="$mount"
        perm="rw"  # Default to read-write
    fi

    # Validate source path
    if [[ -z "$source" ]]; then
        echo "❌ Error: Empty source path in mount specification: '$mount'" >&2
        return 1
    fi

    # Check if source exists and is a directory
    if [[ ! -e "$source" ]]; then
        echo "❌ Error: Path '$source' does not exist" >&2
        echo "💡 Current directory: $(pwd)" >&2
        # shellcheck disable=SC2012  # ls is appropriate here for human-readable error output
        echo "💡 Available paths: $(ls -la 2>/dev/null | head -5 | tail -n +2 | awk '{print $NF}' | tr '\n' ' ')" >&2
        return 1
    fi

    if [[ ! -d "$source" ]]; then
        echo "❌ Error: Path '$source' exists but is not a directory" >&2
        echo "💡 File type: $(file "$source" 2>/dev/null || echo 'unknown')" >&2
        return 1
    fi

    # Validate directory security with enhanced error messages and recovery suggestions
    if ! validate_mount_security "$source"; then
        local security_error_code=$?
        echo "❌ Error: Mount security validation failed for '$source'" >&2
        echo "🔒 Security validation error code: $security_error_code" >&2
        echo "💡 Common causes and solutions:" >&2
        echo "   - Dangerous characters in path → Use only alphanumeric, hyphens, underscores, and slashes" >&2
        echo "   - Path traversal attempts → Avoid '..' sequences and encoded characters" >&2
        echo "   - System-critical directory → Only mount user directories and project files" >&2
        echo "   - Path not in allowed locations → Use directories under /home, /workspace, /tmp, or current directory" >&2
        echo "💡 For paths with special characters, try creating a symbolic link:" >&2
        echo "     ln -s 'problematic,path' safe-path && vm temp safe-path" >&2

        # Log security validation failure for auditing
        if command -v logger >/dev/null 2>&1; then
            logger -t vm-security "SECURITY: Mount validation failed for path: $source (error: $security_error_code)"
        fi

        return 1
    fi

    # Parse permissions and construct mount argument with detailed error handling
    local permission_flags
    if ! permission_flags=$(parse_mount_permissions "$perm"); then
        echo "❌ Error: Failed to parse mount permissions for '$perm'" >&2
        echo "💡 Valid permission values: rw, ro, readwrite, readonly" >&2
        echo "💡 Example: ./src:rw or ./config:ro" >&2
        return 1
    fi

    # Construct mount argument with comprehensive error handling
    local mount_arg
    if ! mount_arg=$(construct_mount_argument "$source" "$permission_flags"); then
        echo "❌ Error: Failed to construct mount argument for '$source'" >&2
        echo "💡 This could indicate:" >&2
        echo "   - Path resolution issues" >&2
        echo "   - Special characters in path" >&2
        echo "   - Permission problems" >&2
        echo "💡 Try using absolute paths or check file permissions" >&2
        return 1
    fi

    echo "$mount_arg"
}

# Parse comma-separated mount string into mount arguments with comprehensive error handling
# Note: Directory names containing commas are not supported due to parsing complexity
parse_mount_string() {
    local mount_str="$1"
    local mount_args=""
    local failed_mounts=()
    local successful_mounts=()

    # Validate input
    if [[ -z "$mount_str" ]]; then
        echo "⚠️ Warning: Empty mount string provided" >&2
        return 0  # Empty is valid, just return empty args
    fi

    # Split by comma and process each mount (save original IFS)
    local old_ifs="$IFS"
    IFS=','
    local MOUNTS
    IFS=',' read -r -a MOUNTS <<< "$mount_str"  # Proper array assignment
    IFS="$old_ifs"

    # Validate we have at least one mount
    if [[ ${#MOUNTS[@]} -eq 0 ]]; then
        echo "❌ Error: No mounts found in mount string: '$mount_str'" >&2
        return 1
    fi

    # Pre-validate: Detect comma-in-directory-name issues
    if ! detect_comma_in_paths MOUNTS; then
        echo "❌ Error: Mount string parsing failed - possible comma in directory names" >&2
        echo "💡 Directory names containing commas are not supported" >&2
        echo "💡 Use symbolic links to work around this: ln -s 'dir,with,commas' dir-no-commas" >&2
        return 1
    fi

    # Process each mount using dedicated sub-function
    for mount in "${MOUNTS[@]}"; do
        local mount_arg
        if mount_arg=$(process_single_mount "$mount"); then
            mount_args="$mount_args $mount_arg"
            successful_mounts+=("$mount")
        else
            failed_mounts+=("$mount")
        fi
    done

    # Report results with detailed error analysis
    if [[ ${#failed_mounts[@]} -gt 0 ]]; then
        echo "❌ Error: Failed to process ${#failed_mounts[@]} mount(s):" >&2
        for failed_mount in "${failed_mounts[@]}"; do
            echo "  ❌ $failed_mount" >&2
        done

        if [[ ${#successful_mounts[@]} -gt 0 ]]; then
            echo "" >&2
            echo "⚠️ Successfully processed ${#successful_mounts[@]} mount(s):" >&2
            for successful_mount in "${successful_mounts[@]}"; do
                echo "  ✅ $successful_mount" >&2
            done
            echo "" >&2
            echo "⚠️ Cannot continue with partial mount failure (security requirement)" >&2
        fi

        echo "💡 Mount processing failed - check the specific error messages above" >&2
        echo "💡 All mount points must be valid for security reasons" >&2
        return 1
    fi

    if [[ ${#successful_mounts[@]} -gt 0 ]] && [[ "${VM_DEBUG:-}" = "true" ]]; then
        echo "✅ Successfully processed ${#successful_mounts[@]} mount(s)" >&2
    fi

    echo "$mount_args"
}