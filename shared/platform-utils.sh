#!/bin/bash
# Cross-Platform Utilities
# Purpose: Provide portable alternatives to platform-specific commands
# No fallbacks - fail fast with clear errors for unsupported platforms

set -e
set -u

# Get OS and architecture for download URLs and command selection
get_platform_info() {
    local os arch
    os=$(uname -s)
    arch=$(uname -m)

    case "$os" in
        Linux)
            case "$arch" in
                x86_64) echo "linux_amd64" ;;
                aarch64|arm64) echo "linux_arm64" ;;
                *)
                    echo "❌ Error: Unsupported Linux architecture: $arch" >&2
                    echo "Supported: x86_64, aarch64/arm64" >&2
                    exit 1
                    ;;
            esac
            ;;
        Darwin)
            case "$arch" in
                x86_64) echo "darwin_amd64" ;;
                arm64) echo "darwin_arm64" ;;
                *)
                    echo "❌ Error: Unsupported macOS architecture: $arch" >&2
                    echo "Supported: x86_64, arm64" >&2
                    exit 1
                    ;;
            esac
            ;;
        *)
            echo "❌ Error: Unsupported operating system: $os" >&2
            echo "Supported platforms: Linux, macOS" >&2
            echo "For Windows, use WSL2 with Linux" >&2
            exit 1
            ;;
    esac
}


# Cross-platform readlink -f equivalent
portable_readlink() {
    local path="$1"

    if [[ "$(uname -s)" == "Darwin" ]]; then
        # macOS doesn't support readlink -f
        if command -v python3 >/dev/null 2>&1; then
            python3 -c "import os,sys; print(os.path.realpath(sys.argv[1]))" "$path"
        else
            echo "❌ Error: Python3 required for macOS file path resolution" >&2
            exit 1
        fi
    else
        # Linux has readlink -f
        readlink -f "$path"
    fi
}

# Cross-platform relative path calculation
portable_relative_path() {
    local base_dir="$1"
    local target_dir="$2"

    if [[ "$(uname -s)" == "Darwin" ]]; then
        # macOS doesn't support realpath --relative-to
        if command -v python3 >/dev/null 2>&1; then
            python3 -c "import os,sys; print(os.path.relpath(sys.argv[2], sys.argv[1]))" "$base_dir" "$target_dir"
        else
            echo "❌ Error: Python3 required for macOS relative path calculation" >&2
            exit 1
        fi
    else
        # Linux has realpath --relative-to
        realpath --relative-to="$base_dir" "$target_dir"
    fi
}

# Cross-platform date parsing to convert ISO 8601 timestamp to epoch seconds
# This function provides portable date parsing between GNU date and BSD date
portable_date_to_epoch() {
    local iso_timestamp="$1"

    # Validate input
    if [[ -z "$iso_timestamp" ]]; then
        echo "❌ Error: Empty timestamp provided for date conversion" >&2
        return 1
    fi

    if [[ "$(uname -s)" == "Darwin" ]]; then
        # macOS doesn't support date -d, use -j with -f for parsing
        if command -v python3 >/dev/null 2>&1; then
            # Use Python for reliable ISO 8601 parsing on macOS
            python3 -c "
import sys
import datetime
import re

timestamp = sys.argv[1]
try:
    # Handle Docker's ISO timestamp format: 2024-01-01T12:34:56.123456789Z
    # Remove nanosecond precision if present, keep microseconds
    timestamp = re.sub(r'\.(\d{6})\d*Z?$', r'.\1', timestamp)
    if timestamp.endswith('Z'):
        timestamp = timestamp[:-1] + '+00:00'

    # Parse ISO format timestamp
    dt = datetime.datetime.fromisoformat(timestamp)
    epoch = int(dt.timestamp())
    print(epoch)
except Exception as e:
    print('ERROR: ' + str(e), file=sys.stderr)
    sys.exit(1)
" "$iso_timestamp"
        else
            echo "❌ Error: Python3 required for date parsing on macOS" >&2
            return 1
        fi
    else
        # Linux has date -d
        date -d "$iso_timestamp" +%s 2>/dev/null
    fi
}