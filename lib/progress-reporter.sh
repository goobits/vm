#!/bin/bash
# Progress Reporter Module
# Provides unified progress reporting for VM operations

# Guard against multiple sourcing
if [[ -n "${PROGRESS_REPORTER_LOADED:-}" ]]; then
    return 0
fi
PROGRESS_REPORTER_LOADED=1

# Terminal width detection
TERM_WIDTH=$(tput cols 2>/dev/null || echo 80)
PROGRESS_WIDTH=$((TERM_WIDTH > 80 ? 80 : TERM_WIDTH))

# Colors and symbols - only set if not already defined
if [[ -z "${PROGRESS_COLORS_DEFINED:-}" ]]; then
    PROGRESS_COLORS_DEFINED=1
    RESET='\033[0m'
    BOLD='\033[1m'
    DIM='\033[2m'
    GREEN='\033[32m'
    YELLOW='\033[33m'
    BLUE='\033[34m'
    RED='\033[31m'
    CHECK='✓'
    CROSS='✗'
    DOTS='.'
fi

# Progress tracking variables
declare -A PROGRESS_TASKS=()
declare -A PROGRESS_STATUS=()
PROGRESS_INDENT_LEVEL=0
PROGRESS_LAST_LINE=""
PROGRESS_IN_PROGRESS=""

# Initialize progress reporter
progress_init() {
    local title="$1"
    local vm_name="${2:-}"
    
    if [[ -n "$vm_name" ]]; then
        echo -e "${BOLD}🔄 VM Operation: ${vm_name}${RESET}"
    else
        echo -e "${BOLD}${title}${RESET}"
    fi
    
    # Print separator line using ASCII characters for better compatibility
    printf '%*s\n' "$PROGRESS_WIDTH" '' | tr ' ' '='
    echo
}

# Start a new phase
progress_phase() {
    local icon="$1"
    local title="$2"
    local prefix="${3:-├─}"
    
    # Clear any in-progress status
    if [[ -n "$PROGRESS_IN_PROGRESS" ]]; then
        progress_done
    fi
    
    echo -e "${prefix} ${icon} ${BOLD}${title}${RESET}"
    PROGRESS_INDENT_LEVEL=1
}

# Start a task
progress_task() {
    local title="$1"
    local is_subtask="${2:-false}"
    
    # Clear any in-progress status
    if [[ -n "$PROGRESS_IN_PROGRESS" ]]; then
        progress_done
    fi
    
    local indent=""
    local prefix="├─"
    
    if [[ "$is_subtask" == "true" ]]; then
        indent="   "
        prefix="├─"
        PROGRESS_INDENT_LEVEL=2
    else
        prefix="├─"
        PROGRESS_INDENT_LEVEL=1
    fi
    
    # For tasks that will show progress
    printf "${indent}${prefix} %s " "$title"
    PROGRESS_IN_PROGRESS="$title"
    PROGRESS_LAST_LINE="${indent}${prefix} ${title}"
}

# Update progress dots
progress_update() {
    local dots="${1:-$DOTS}"
    printf "%s" "$dots"
}

# Complete current task
progress_done() {
    if [[ -n "$PROGRESS_IN_PROGRESS" ]]; then
        echo -e " ${GREEN}${CHECK}${RESET}"
        PROGRESS_IN_PROGRESS=""
    fi
}

# Fail current task
progress_fail() {
    local error="${1:-}"
    
    if [[ -n "$PROGRESS_IN_PROGRESS" ]]; then
        echo -e " ${RED}${CROSS}${RESET}"
        if [[ -n "$error" ]]; then
            local indent=""
            for ((i=0; i<PROGRESS_INDENT_LEVEL; i++)); do
                indent="${indent}   "
            done
            echo -e "${indent}${RED}└─ Error: ${error}${RESET}"
        fi
        PROGRESS_IN_PROGRESS=""
    fi
}

# Show a completed subtask
progress_subtask_done() {
    local title="$1"
    local indent="   "
    
    echo -e "${indent}├─ ${GREEN}${CHECK}${RESET} ${title}"
}

# Complete a phase
progress_phase_done() {
    local message="${1:-}"
    local prefix="${2:-└─}"
    
    # Clear any in-progress status
    if [[ -n "$PROGRESS_IN_PROGRESS" ]]; then
        progress_done
    fi
    
    if [[ -n "$message" ]]; then
        echo -e "${prefix} ${GREEN}✅${RESET} ${message}"
    fi
    echo
    PROGRESS_INDENT_LEVEL=0
}

# Show final summary
progress_complete() {
    local message="${1:-Operation complete}"
    local time="${2:-}"
    
    # Clear any in-progress status
    if [[ -n "$PROGRESS_IN_PROGRESS" ]]; then
        progress_done
    fi
    
    # Print separator line using ASCII characters for better compatibility
    printf '%*s\n' "$PROGRESS_WIDTH" '' | tr ' ' '='
    
    if [[ -n "$time" ]]; then
        echo -e "${GREEN}✨${RESET} ${message} (Total time: ${time})"
    else
        echo -e "${GREEN}✨${RESET} ${message}"
    fi
}

# Error handler
progress_error() {
    local message="$1"
    
    # Clear any in-progress status
    if [[ -n "$PROGRESS_IN_PROGRESS" ]]; then
        progress_fail
    fi
    
    echo -e "${RED}❌ Error: ${message}${RESET}"
}

# Progress tracking for external commands
progress_run() {
    local title="$1"
    shift
    
    progress_task "$title"
    
    # Run command and capture output
    local output
    local exit_code
    
    output=$("$@" 2>&1)
    exit_code=$?
    
    if [[ $exit_code -eq 0 ]]; then
        progress_done
    else
        progress_fail
        echo "$output" | sed 's/^/   /'
        return $exit_code
    fi
}

# Helper for multi-line status updates (e.g., for Docker Compose output)
progress_multiline() {
    local phase="$1"
    local current="$2"
    local total="$3"
    
    # Clear current line and print status
    printf "\r%s" "$(printf ' %.0s' {1..80})"  # Clear line
    printf "\r   ├─ %s (%d/%d)" "$phase" "$current" "$total"
}

# Export all functions
export -f progress_init progress_phase progress_task progress_update progress_done
export -f progress_fail progress_subtask_done progress_phase_done progress_complete
export -f progress_error progress_run progress_multiline