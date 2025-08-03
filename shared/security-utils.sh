#!/bin/bash
# Shared Security Utilities
# Extracted from vm.sh and vm-temp.sh for use by both Docker and Vagrant providers
#
# This module provides provider-agnostic security validation utilities
# for path validation, name sanitization, and general security checks.

# Validate directory name for security (basic validation for temp VM usage)
# This is a lighter validation compared to full mount security validation
validate_directory_name() {
    local dir="$1"
    
    # Check for dangerous characters that could cause shell injection
    if [[ "$dir" =~ [\;\`\$\"] ]]; then
        echo "❌ Error: Directory name contains potentially dangerous characters"
        echo "💡 Directory names cannot contain: ; \` $ \""
        return 1
    fi
    
    # Check for directory traversal attempts
    if [[ "$dir" =~ (\.\./|/\.\.) ]]; then
        echo "❌ Error: Directory path traversal not allowed"
        return 1
    fi
    
    return 0
}

# Sanitize project name by removing potentially dangerous characters
# This function extracts only alphanumeric characters from a project name
sanitize_project_name() {
    local project_name="$1"
    
    # Validate input
    if [[ -z "$project_name" ]]; then
        echo "❌ Error: Empty project name provided" >&2
        return 1
    fi
    
    # Extract only alphanumeric characters (removes all special chars)
    local sanitized_name
    sanitized_name=$(echo "$project_name" | tr -cd '[:alnum:]')
    
    # Validate that we still have something after sanitization
    if [[ -z "$sanitized_name" ]]; then
        echo "❌ Error: Project name contains no valid characters after sanitization" >&2
        echo "💡 Project names must contain at least one alphanumeric character" >&2
        return 1
    fi
    
    echo "$sanitized_name"
}

# Validate that a path exists and is secure for basic operations
# This is a general validation that can be used for various path checks
validate_path_basic() {
    local path="$1"
    local path_type="${2:-file}"  # 'file', 'directory', or 'any'
    
    # Validate input
    if [[ -z "$path" ]]; then
        echo "❌ Error: Empty path provided" >&2
        return 1
    fi
    
    # Check if path exists
    if [[ ! -e "$path" ]]; then
        echo "❌ Error: Path '$path' does not exist" >&2
        return 1
    fi
    
    # Check path type if specified
    case "$path_type" in
        "file")
            if [[ ! -f "$path" ]]; then
                echo "❌ Error: Path '$path' is not a regular file" >&2
                return 1
            fi
            ;;
        "directory")
            if [[ ! -d "$path" ]]; then
                echo "❌ Error: Path '$path' is not a directory" >&2
                return 1
            fi
            ;;
        "any")
            # Accept any type of existing path
            ;;
        *)
            echo "❌ Error: Invalid path type '$path_type' specified" >&2
            echo "💡 Valid types: file, directory, any" >&2
            return 1
            ;;
    esac
    
    return 0
}

# Check for dangerous shell metacharacters in strings
# This is a general utility for validating user input
check_dangerous_characters() {
    local input_string="$1"
    local context="${2:-general}"  # Context for better error messages
    
    # Check for dangerous shell metacharacters
    case "$input_string" in
        *\;* | *\`* | *\$* | *\"* | *\|* | *\&* | *\>* | *\<* | *\(* | *\)* | *\{* | *\}* | *\** | *\?* | *\[* | *\]* | *~* | *@* | *#* | *%*)
            echo "❌ Error: $context contains potentially dangerous characters" >&2
            echo "💡 The following characters are not allowed: ; \` $ \" | & > < ( ) { } * ? [ ] ~ @ # %" >&2
            return 1
            ;;
    esac
    
    # Check for dangerous control characters using length check
    local clean_string="${input_string//[$'\0\n\r\t']/}"
    if [[ ${#clean_string} -ne ${#input_string} ]]; then
        echo "❌ Error: $context contains dangerous control characters" >&2
        echo "💡 Control characters (null, newline, carriage return, tab) are not allowed" >&2
        return 1
    fi
    
    return 0
}

# Validate and resolve a path to its canonical form
# This function provides secure path resolution with validation
resolve_path_secure() {
    local input_path="$1"
    local must_exist="${2:-true}"  # Whether the path must exist
    
    # Validate input
    if [[ -z "$input_path" ]]; then
        echo "❌ Error: Empty path provided for resolution" >&2
        return 1
    fi
    
    # Check for dangerous characters first
    if ! check_dangerous_characters "$input_path" "path"; then
        return 1
    fi
    
    # Attempt to resolve the path
    local resolved_path
    if [[ "$must_exist" == "true" ]]; then
        # Path must exist for resolution
        if ! resolved_path=$(realpath "$input_path" 2>/dev/null); then
            echo "❌ Error: Cannot resolve path '$input_path' (path may not exist)" >&2
            return 1
        fi
    else
        # Allow non-existing paths, resolve parent directory
        local parent_dir
        local filename
        parent_dir=$(dirname "$input_path")
        filename=$(basename "$input_path")
        
        if ! parent_dir=$(realpath "$parent_dir" 2>/dev/null); then
            echo "❌ Error: Cannot resolve parent directory of '$input_path'" >&2
            return 1
        fi
        
        resolved_path="$parent_dir/$filename"
    fi
    
    # Validate the resolved path doesn't contain dangerous patterns
    if [[ "$resolved_path" == *".."* ]]; then
        echo "❌ Error: Resolved path contains parent directory references" >&2
        echo "💡 Path: $resolved_path" >&2
        return 1
    fi
    
    echo "$resolved_path"
}

# Check if a path is within allowed directories (basic whitelist)
# This provides a simple whitelist check for common safe directories
is_path_in_safe_locations() {
    local check_path="$1"
    local additional_safe_paths="${2:-}"  # Optional additional safe paths (space-separated)
    
    # Validate input
    if [[ -z "$check_path" ]]; then
        echo "❌ Error: Empty path provided for safety check" >&2
        return 1
    fi
    
    # Resolve the path to its canonical form
    local resolved_path
    if ! resolved_path=$(resolve_path_secure "$check_path" "true"); then
        return 1
    fi
    
    # Define basic safe path prefixes
    local safe_path_prefixes=(
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
    
    # Add current working directory as safe
    local current_dir
    current_dir=$(pwd)
    safe_path_prefixes+=("$current_dir/")
    
    # Add any additional safe paths provided
    if [[ -n "$additional_safe_paths" ]]; then
        # Split additional paths by space and add to safe list
        for additional_path in $additional_safe_paths; do
            safe_path_prefixes+=("$additional_path")
        done
    fi
    
    # Check if the path is in a safe location
    for safe_prefix in "${safe_path_prefixes[@]}"; do
        if [[ "$resolved_path" == "$safe_prefix"* ]] || [[ "$resolved_path" == "${safe_prefix%/}" ]]; then
            return 0  # Path is safe
        fi
    done
    
    # Path is not in any safe location
    echo "❌ Error: Path '$resolved_path' is not in allowed safe locations" >&2
    echo "💡 Safe locations include:" >&2
    printf "   %s\n" "${safe_path_prefixes[@]}" >&2
    return 1
}

# Log security events to system logger if available
# This provides centralized security event logging
log_security_event() {
    local event_type="$1"    # "INFO", "WARN", "ERROR", "CRITICAL"
    local component="$2"     # Component name (e.g., "vm-mount", "vm-temp")
    local message="$3"       # The security event message
    local details="${4:-}"   # Optional additional details
    
    # Validate required parameters
    if [[ -z "$event_type" ]] || [[ -z "$component" ]] || [[ -z "$message" ]]; then
        echo "❌ Error: Missing required parameters for security logging" >&2
        return 1
    fi
    
    # Construct the log message
    local log_message="[$event_type] $component: $message"
    if [[ -n "$details" ]]; then
        log_message="$log_message - Details: $details"
    fi
    
    # Log to system logger if available
    if command -v logger >/dev/null 2>&1; then
        case "$event_type" in
            "CRITICAL"|"ERROR")
                logger -t vm-security -p user.err "$log_message"
                ;;
            "WARN")
                logger -t vm-security -p user.warning "$log_message"
                ;;
            "INFO")
                logger -t vm-security -p user.info "$log_message"
                ;;
            *)
                logger -t vm-security "$log_message"
                ;;
        esac
    fi
    
    # Also log to stderr if debug mode is enabled
    if [[ "${VM_DEBUG:-}" = "true" ]]; then
        echo "🔒 SECURITY LOG: $log_message" >&2
    fi
    
    return 0
}

# Validate that a string contains only safe characters for identifiers
# This is useful for validating container names, project names, etc.
validate_safe_identifier() {
    local identifier="$1"
    local identifier_type="${2:-identifier}"  # Type for error messages
    local allow_dashes="${3:-true}"           # Whether to allow dashes
    local allow_underscores="${4:-true}"      # Whether to allow underscores
    
    # Validate input
    if [[ -z "$identifier" ]]; then
        echo "❌ Error: Empty $identifier_type provided" >&2
        return 1
    fi
    
    # Check length (reasonable limits)
    if [[ ${#identifier} -gt 64 ]]; then
        echo "❌ Error: $identifier_type is too long (max 64 characters)" >&2
        return 1
    fi
    
    if [[ ${#identifier} -lt 1 ]]; then
        echo "❌ Error: $identifier_type is too short (min 1 character)" >&2
        return 1
    fi
    
    # Build allowed character pattern
    local pattern="^[a-zA-Z0-9"
    
    if [[ "$allow_dashes" == "true" ]]; then
        pattern="${pattern}-"
    fi
    
    if [[ "$allow_underscores" == "true" ]]; then
        pattern="${pattern}_"
    fi
    
    pattern="${pattern}]+$"
    
    # Validate against pattern
    if [[ ! "$identifier" =~ $pattern ]]; then
        echo "❌ Error: $identifier_type contains invalid characters" >&2
        echo "💡 Only alphanumeric characters$(
            if [[ "$allow_dashes" == "true" ]] && [[ "$allow_underscores" == "true" ]]; then
                echo ", dashes, and underscores"
            elif [[ "$allow_dashes" == "true" ]]; then
                echo " and dashes"
            elif [[ "$allow_underscores" == "true" ]]; then
                echo " and underscores"
            fi
        ) are allowed" >&2
        return 1
    fi
    
    # Ensure it doesn't start with a dash or underscore (common requirement)
    if [[ "$identifier" =~ ^[-_] ]]; then
        echo "❌ Error: $identifier_type cannot start with a dash or underscore" >&2
        return 1
    fi
    
    return 0
}