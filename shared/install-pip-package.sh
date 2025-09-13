#!/bin/bash
# Install pip package with smart detection (pipx for CLI tools, pip for libraries)
# Usage: install-pip-package.sh <project_user> <package>

set -e

PROJECT_USER="$1"
PACKAGE="$2"

if [[ -z "$PROJECT_USER" || -z "$PACKAGE" ]]; then
    echo "Usage: $0 <project_user> <package>"
    exit 1
fi

export PATH="$HOME/.local/bin:$PATH"

# Sanitize package name to match mount path (replace - with _ and handle case)
safe_package_name="${PACKAGE//-/_}"
safe_package_name="${safe_package_name,,}"

# Check if this is a linked local package
if [ -d "/home/${PROJECT_USER}/.links/pip/$PACKAGE" ] || [ -d "/home/${PROJECT_USER}/.links/pip/$safe_package_name" ]; then
    # Local packages are always installed with pip in editable mode
    if [ -d "/home/${PROJECT_USER}/.links/pip/$PACKAGE" ]; then
        echo "🔗 Linking local package with pip: $PACKAGE"
        python3 -m pip install --user --break-system-packages -e "/home/${PROJECT_USER}/.links/pip/$PACKAGE"
    else
        echo "🔗 Linking local package with pip: $safe_package_name"
        python3 -m pip install --user --break-system-packages -e "/home/${PROJECT_USER}/.links/pip/$safe_package_name"
    fi
else
    # For registry packages, try pipx first (for CLI tools), fallback to pip (for libraries)
    echo "📦 Attempting to install $PACKAGE from registry..."

    # First try with pipx (good for CLI tools)
    pipx_error_file="$(mktemp /tmp/pipx_error_XXXXXX.log)"
    if /usr/bin/pipx install "$PACKAGE" 2>"$pipx_error_file"; then
        echo "✅ Installed $PACKAGE as CLI tool with pipx"
    else
        # Check if it failed because it's a library (not a CLI tool)
        if grep -q "No apps associated with package\|not a valid package\|library" "$pipx_error_file" 2>/dev/null; then
            echo "📚 $PACKAGE appears to be a library, installing with pip..."
            if python3 -m pip install --user --break-system-packages "$PACKAGE"; then
                echo "✅ Installed $PACKAGE as library with pip"
            else
                echo "❌ Failed to install $PACKAGE with both pipx and pip"
                rm -f "$pipx_error_file"
                exit 1
            fi
        else
            # Some other pipx error - show it and fail
            cat "$pipx_error_file"
            rm -f "$pipx_error_file"
            exit 1
        fi
    fi
    rm -f "$pipx_error_file"
fi