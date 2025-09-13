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
# Determine the correct linked directory path
LINKED_DIR=""
if [ -d "/home/${PROJECT_USER}/.links/pip/$PACKAGE" ]; then
    LINKED_DIR="/home/${PROJECT_USER}/.links/pip/$PACKAGE"
elif [ -d "/home/${PROJECT_USER}/.links/pip/$safe_package_name" ]; then
    LINKED_DIR="/home/${PROJECT_USER}/.links/pip/$safe_package_name"
fi

if [ -n "$LINKED_DIR" ]; then
    echo "🔗 Found linked local package: $PACKAGE"

    # Check if it's a pipx environment by looking for pipx_metadata.json
    if [ -f "$LINKED_DIR/pipx_metadata.json" ]; then
        echo "  -> Detected as a pipx environment."
        LINKED_BIN_DIR="$LINKED_DIR/bin"
        LOCAL_BIN_DIR="/home/${PROJECT_USER}/.local/bin"
        
        mkdir -p "$LOCAL_BIN_DIR"

        echo "  -> Creating wrapper scripts in $LOCAL_BIN_DIR to use the VM's python interpreter."
        for SCRIPT_PATH in "$LINKED_BIN_DIR"/*; do
            if [ -f "$SCRIPT_PATH" ] && [ -x "$SCRIPT_PATH" ]; then
                SCRIPT_NAME=$(basename "$SCRIPT_PATH")
                WRAPPER_PATH="$LOCAL_BIN_DIR/$SCRIPT_NAME"
                
                echo "    - Wrapping command: $SCRIPT_NAME"
                
                # Create a MORE ROBUST wrapper script
                cat > "$WRAPPER_PATH" << 'WRAPPER_EOF'
#!/bin/sh
# VM-Tool generated wrapper for linked pipx package
set -e

LINKED_DIR="__LINKED_DIR__"
SCRIPT_PATH="__SCRIPT_PATH__"

# Find site-packages with multiple strategies
SITE_PACKAGES=""

# Strategy 1: Look for standard python version paths
for pydir in "$LINKED_DIR"/lib/python*/site-packages; do
    if [ -d "$pydir" ]; then
        SITE_PACKAGES="$pydir"
        break
    fi
done

# Strategy 2: Use find as fallback
if [ -z "$SITE_PACKAGES" ]; then
    SITE_PACKAGES=$(find "$LINKED_DIR" -type d -name "site-packages" 2>/dev/null | head -1)
fi

# Strategy 3: Check if there's a venv structure
if [ -z "$SITE_PACKAGES" ] && [ -d "$LINKED_DIR/lib" ]; then
    # Sometimes it's just lib/site-packages without python version
    if [ -d "$LINKED_DIR/lib/site-packages" ]; then
        SITE_PACKAGES="$LINKED_DIR/lib/site-packages"
    fi
fi

# Export PYTHONPATH if we found site-packages
if [ -n "$SITE_PACKAGES" ]; then
    export PYTHONPATH="$SITE_PACKAGES:${PYTHONPATH:-}"
    # Also add the linked dir itself, some packages need it
    export PYTHONPATH="$LINKED_DIR:$PYTHONPATH"
fi

# Execute the script with python3, completely ignoring the shebang
exec python3 "$SCRIPT_PATH" "$@"
WRAPPER_EOF

                # Replace placeholders with actual paths
                sed -i "s|__LINKED_DIR__|$LINKED_DIR|g" "$WRAPPER_PATH"
                sed -i "s|__SCRIPT_PATH__|$SCRIPT_PATH|g" "$WRAPPER_PATH"
                
                chmod +x "$WRAPPER_PATH"
            fi
        done
        echo "  -> Wrapper scripts created. Please restart your shell to use them."

    # Check if it's a standard python project (with setup.py or pyproject.toml)
    elif [ -f "$LINKED_DIR/setup.py" ] || [ -f "$LINKED_DIR/pyproject.toml" ]; then
        echo "  -> Detected as a source project. Installing in editable mode."
        python3 -m pip install --user --break-system-packages -e "$LINKED_DIR"
    
    # Fallback for directories that are neither
    else
        echo "  -> ⚠️  Warning: Linked directory is not a recognized pipx environment or Python source project."
        echo "  -> Attempting editable install as a fallback..."
        python3 -m pip install --user --break-system-packages -e "$LINKED_DIR"
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