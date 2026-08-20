#!/bin/bash
set -euo pipefail

# vm-worktree - Manage git worktrees easily
# Part of the vm tool: https://github.com/goobits/vm
#
# This script provides a user-friendly interface for creating and managing
# git worktrees inside containers, with full support for host access.

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Validate VM_WORKTREES is set
if [ -z "${VM_WORKTREES:-}" ]; then
    echo -e "${RED}Error: VM_WORKTREES environment variable not set${NC}" >&2
    echo "This should be set automatically. Try restarting your shell." >&2
    exit 1
fi

# Validate VM_WORKTREES path safety (prevent override to dangerous locations)
case "$VM_WORKTREES" in
    /etc/*|/bin/*|/sbin/*|/usr/bin/*|/usr/sbin/*|/boot/*|/sys/*|/proc/*)
        echo -e "${RED}Error: VM_WORKTREES points to dangerous system directory: $VM_WORKTREES${NC}" >&2
        echo "This may indicate a configuration error or security issue." >&2
        exit 1
        ;;
    /*)
        # Absolute path is good, continue
        ;;
    *)
        echo -e "${RED}Error: VM_WORKTREES must be an absolute path: $VM_WORKTREES${NC}" >&2
        exit 1
        ;;
esac

# Check if VM_WORKTREES is writable
if [ ! -w "$VM_WORKTREES" ]; then
    echo -e "${RED}Error: No write permission to $VM_WORKTREES${NC}" >&2
    echo "Check container configuration or directory permissions" >&2
    exit 1
fi

# Helper: Comprehensive name sanitization with validation
sanitize_name() {
    local name="$1"

    # Check for empty
    if [ -z "$name" ]; then
        echo -e "${RED}Error: Worktree name cannot be empty${NC}" >&2
        return 1
    fi

    # Check for . or ..
    if [[ "$name" == "." ]] || [[ "$name" == ".." ]]; then
        echo -e "${RED}Error: Invalid name '$name'${NC}" >&2
        return 1
    fi

    # Check for path traversal sequences
    if [[ "$name" == *".."* ]] || [[ "$name" == *"/./"* ]] || [[ "$name" == *//* ]]; then
        echo -e "${RED}Error: Name contains path traversal sequences${NC}" >&2
        return 1
    fi

    # Remove dangerous characters (keep only alphanumeric, dash, underscore, slash)
    local clean=$(echo "$name" | sed 's/[^a-zA-Z0-9/_-]//g')

    # Check if name was all invalid characters
    if [ -z "$clean" ]; then
        echo -e "${RED}Error: Name contains only invalid characters${NC}" >&2
        return 1
    fi

    # Check length (be conservative for cross-platform compatibility)
    if [ ${#clean} -gt 200 ]; then
        echo -e "${RED}Error: Name too long (max 200 characters)${NC}" >&2
        return 1
    fi

    echo "$clean"
}

# Helper: Validate that resolved path is within VM_WORKTREES (prevent traversal)
validate_worktree_path() {
    local path="$1"

    # Resolve to absolute path (follow symlinks)
    local resolved
    if ! resolved=$(realpath -m "$path" 2>/dev/null); then
        echo -e "${RED}Error: Cannot resolve path: $path${NC}" >&2
        return 1
    fi

    # Ensure it's under VM_WORKTREES
    local base_resolved
    if ! base_resolved=$(realpath -m "$VM_WORKTREES" 2>/dev/null); then
        echo -e "${RED}Error: Cannot resolve base path: $VM_WORKTREES${NC}" >&2
        return 1
    fi

    # Check if resolved path starts with base path
    if [[ "$resolved" != "$base_resolved"* ]]; then
        echo -e "${RED}Error: Path escapes worktrees directory${NC}" >&2
        echo "  Attempted: $path" >&2
        echo "  Resolved: $resolved" >&2
        echo "  Expected under: $base_resolved" >&2
        return 1
    fi

    return 0
}

# Helper: Check if in git repo
check_git_repo() {
    if ! git rev-parse --git-dir > /dev/null 2>&1; then
        echo -e "${RED}Error: Not in a git repository${NC}" >&2
        echo "cd to /workspace or a git repository first" >&2
        exit 1
    fi
}

# Helper: Check if running interactively (not in pipeline/background/subshell)
is_interactive() {
    # Check stdin/stdout are TTYs, not in background, not in pipe
    [[ -t 0 ]] && [[ -t 1 ]] && [[ ! -p /dev/stdin ]] && [[ ! -p /dev/stdout ]]
}

# Helper: Safe shell execution with fallback
safe_exec_shell() {
    local target_dir="$1"

    cd "$target_dir" || return 1

    if is_interactive; then
        # Interactive: spawn new shell
        if [ -x "${SHELL:-}" ]; then
            exec "$SHELL"
        elif [ -x /bin/bash ]; then
            exec /bin/bash
        elif [ -x /bin/sh ]; then
            exec /bin/sh
        else
            echo -e "${YELLOW}Warning: No valid shell found, staying in current shell${NC}" >&2
        fi
    else
        # Non-interactive (scripted/chained): just cd and continue
        echo "  Now in: $target_dir"
    fi
}

# Cleanup function for interrupted operations
cleanup_partial_worktree() {
    if [ -n "${WORKTREE_PATH:-}" ] && [ -d "$WORKTREE_PATH" ]; then
        echo -e "${YELLOW}Cleaning up partial worktree...${NC}" >&2
        git worktree remove --force "$WORKTREE_PATH" 2>/dev/null || rm -rf "$WORKTREE_PATH"
    fi
}

case "${1:-help}" in
    add|create)
        check_git_repo

        if [ -z "${2:-}" ]; then
            echo -e "${RED}Error: Worktree name required${NC}" >&2
            echo "Usage: vm-worktree add <name> [branch]" >&2
            exit 1
        fi

        # Sanitize and validate name
        NAME=$(sanitize_name "$2") || exit 1
        if [ "$NAME" != "$2" ]; then
            echo -e "${YELLOW}Warning: Sanitized name '$2' → '$NAME'${NC}"
        fi

        WORKTREE_PATH="$VM_WORKTREES/$NAME"

        # Validate path doesn't escape base directory
        validate_worktree_path "$WORKTREE_PATH" || exit 1

        BRANCH="${3:-$NAME}"

        # Validate branch name (prevent injection with leading dashes)
        if [[ "$BRANCH" == -* ]]; then
            echo -e "${RED}Error: Branch name cannot start with '-'${NC}" >&2
            echo "If you meant a branch with a dash, quote it: vm-worktree add name \"$BRANCH\"" >&2
            exit 1
        fi

        # Set up cleanup trap for CTRL+C or errors
        trap cleanup_partial_worktree EXIT INT TERM

        # Edge case: Worktree already exists
        if [ -d "$WORKTREE_PATH" ]; then
            if git worktree list | grep -q "$WORKTREE_PATH"; then
                echo -e "${GREEN}✓ Worktree '$NAME' already exists${NC}"
                echo "  Navigating to: $WORKTREE_PATH"
                trap - EXIT INT TERM  # Clear trap
                safe_exec_shell "$WORKTREE_PATH"
                exit 0
            else
                echo -e "${YELLOW}Warning: Directory exists but not a worktree${NC}"
                if is_interactive; then
                    read -p "Remove and recreate? (y/N): " -n 1 -r
                    echo
                    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
                        echo "Cancelled"
                        trap - EXIT INT TERM
                        exit 0
                    fi
                fi
                rm -rf "$WORKTREE_PATH"
            fi
        fi

        # Try to create worktree (use -- to separate branch name from flags)
        echo "Creating worktree: $NAME (branch: $BRANCH)"
        if git worktree add "$WORKTREE_PATH" -- "$BRANCH" 2>&1; then
            echo -e "${GREEN}✓ Worktree created: $NAME${NC}"
            trap - EXIT INT TERM  # Clear trap on success
            safe_exec_shell "$WORKTREE_PATH"
        else
            echo -e "${RED}✗ Failed to create worktree${NC}" >&2
            exit 1
        fi
        ;;

    list|ls)
        echo -e "${GREEN}📁 Worktrees:${NC}"
        if [ -d "$VM_WORKTREES" ] && [ "$(ls -A "$VM_WORKTREES" 2>/dev/null)" ]; then
            git worktree list 2>/dev/null | grep "$VM_WORKTREES" || echo "  (none in $VM_WORKTREES)"
        else
            echo "  (none yet - use 'vm-worktree add <name>')"
        fi
        ;;

    remove|rm)
        check_git_repo

        if [ -z "${2:-}" ]; then
            echo -e "${RED}Error: Worktree name required${NC}" >&2
            echo "Usage: vm-worktree remove <name>" >&2
            vm-worktree list
            exit 1
        fi

        NAME=$(sanitize_name "$2") || exit 1
        WORKTREE_PATH="$VM_WORKTREES/$NAME"

        # Validate path doesn't escape base directory
        validate_worktree_path "$WORKTREE_PATH" || exit 1

        if [ ! -d "$WORKTREE_PATH" ]; then
            echo -e "${YELLOW}Warning: Worktree '$NAME' not found${NC}"
            vm-worktree list
            exit 1
        fi

        if git worktree remove "$WORKTREE_PATH" 2>&1; then
            echo -e "${GREEN}✓ Worktree removed: $NAME${NC}"
        else
            echo -e "${RED}✗ Failed to remove worktree${NC}" >&2
            echo "Try: git worktree remove --force $WORKTREE_PATH" >&2
            exit 1
        fi
        ;;

    goto|cd)
        if [ -z "${2:-}" ]; then
            safe_exec_shell "$VM_WORKTREES"
        else
            NAME=$(sanitize_name "$2") || exit 1
            WORKTREE_PATH="$VM_WORKTREES/$NAME"

            # Validate path doesn't escape base directory
            validate_worktree_path "$WORKTREE_PATH" || exit 1

            if [ -d "$WORKTREE_PATH" ]; then
                safe_exec_shell "$WORKTREE_PATH"
            else
                echo -e "${RED}Error: Worktree '$NAME' not found${NC}" >&2
                vm-worktree list
                exit 1
            fi
        fi
        ;;

    help|--help|-h|*)
        echo "vm-worktree - Manage git worktrees easily"
        echo ""
        echo "Usage:"
        echo "  vm-worktree add <name> [branch]    Create worktree (defaults to branch=name)"
        echo "  vm-worktree list                   List all worktrees"
        echo "  vm-worktree remove <name>          Remove a worktree"
        echo "  vm-worktree goto [name]            Navigate to worktrees dir or specific worktree"
        echo ""
        echo "Examples:"
        echo "  vm-worktree add feature-x          Create 'feature-x' worktree from branch 'feature-x'"
        echo "  vm-worktree add bugfix-1 main      Create 'bugfix-1' from 'main' branch"
        echo "  vm-worktree list                   See all worktrees"
        echo "  vm-worktree goto feature-x         Jump to feature-x worktree"
        echo ""
        echo "Worktrees location: $VM_WORKTREES"
        ;;
esac
