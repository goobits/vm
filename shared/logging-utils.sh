#!/bin/bash
# Shared Logging Utilities
# Simple, container-friendly structured logging for shell scripts

# Guard against multiple sourcing
if [[ -n "${LOGGING_UTILS_LOADED:-}" ]]; then
    return 0
fi
LOGGING_UTILS_LOADED=1

# Get log level numeric value for filtering
log_level_num() {
    case "${LOG_LEVEL:-INFO}" in
        DEBUG) echo 0 ;;
        INFO)  echo 1 ;;
        WARN)  echo 2 ;;
        ERROR) echo 3 ;;
        *) echo 1 ;;  # Default to INFO
    esac
}

# Core logging function
# Usage: vm_log LEVEL "message" "key=value key2=value2"
vm_log() {
    local level="$1"
    local message="$2"
    local context="$3"  # Optional key=value pairs
    
    # Level filtering
    local level_num
    case "$level" in
        DEBUG) level_num=0 ;;
        INFO)  level_num=1 ;;
        WARN)  level_num=2 ;;
        ERROR) level_num=3 ;;
        *) return 1 ;;  # Invalid level
    esac
    
    if [[ $level_num -lt $(log_level_num) ]]; then
        return 0
    fi
    
    # Build log entry: timestamp | level | message | context
    local timestamp=$(date -u +"%Y-%m-%dT%H:%M:%S")
    local output="$timestamp | $level | $message"
    
    if [[ -n "$context" ]]; then
        output="$output | $context"
    fi
    
    # Route to stdout/stderr appropriately for containers
    if [[ "$level" =~ ^(WARN|ERROR)$ ]]; then
        echo "$output" >&2
    else
        echo "$output"
    fi
}

# Convenience functions for common log levels
vm_debug() { vm_log "DEBUG" "$1" "$2"; }
vm_info()  { vm_log "INFO" "$1" "$2"; }
vm_warn()  { vm_log "WARN" "$1" "$2"; }
vm_error() { vm_log "ERROR" "$1" "$2"; }

# Helper to format context from variables
# Usage: vm_info "message" "$(vm_context "key1=$value1" "key2=$value2")"
vm_context() {
    local IFS=" "
    echo "$*"
}

# Standardized error message functions
# These functions provide consistent error formatting across the codebase
# Usage: log_error "Failed to do something" "key=value"
log_error() {
    local message="$1"
    local context="${2:-}"
    echo "❌ Error: $message" >&2
    if [[ -n "$context" ]]; then
        vm_error "$message" "$context"
    else
        vm_error "$message"
    fi
}

# Standardized tip/hint message function
# Usage: log_tip "Try using the --help flag"
log_tip() {
    local tip="$1"
    echo "💡 $tip" >&2
}

# Standardized warning message function
# Usage: log_warning "This may cause issues"
log_warning() {
    local warning="$1"
    local context="${2:-}"
    echo "⚠️ Warning: $warning" >&2
    if [[ -n "$context" ]]; then
        vm_warn "$warning" "$context"
    else
        vm_warn "$warning"
    fi
}

# Standardized success message function
# Usage: log_success "Operation completed successfully"
log_success() {
    local message="$1"
    local context="${2:-}"
    echo "✅ $message"
    if [[ -n "$context" ]]; then
        vm_info "$message" "$context"
    else
        vm_info "$message"
    fi
}

# Standardized info message function with icon
# Usage: log_info_icon "📄" "File created successfully"
log_info_icon() {
    local icon="$1"
    local message="$2"
    local context="${3:-}"
    echo "$icon $message"
    if [[ -n "$context" ]]; then
        vm_info "$message" "$context"
    else
        vm_info "$message"
    fi
}