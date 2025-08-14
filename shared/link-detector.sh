#!/bin/bash
# Generic package linking detector for VM configuration
# Replaces npm-specific linking with support for npm, pip, and cargo
# Provides security-hardened detection with whitelist validation

set -e

# Source existing security utilities
DETECTOR_SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$DETECTOR_SCRIPT_DIR/security-utils.sh"

# Whitelist of allowed package managers for security
declare -ra ALLOWED_PACKAGE_MANAGERS=("npm" "pip" "cargo")

# Validate package manager is in whitelist
validate_package_manager() {
    local pm="$1"
    
    for allowed_pm in "${ALLOWED_PACKAGE_MANAGERS[@]}"; do
        if [[ "$pm" == "$allowed_pm" ]]; then
            return 0
        fi
    done
    
    echo "❌ Error: Package manager '$pm' not in whitelist: ${ALLOWED_PACKAGE_MANAGERS[*]}" >&2
    return 1
}

# Detect npm linked packages (maintains existing functionality)
detect_npm_packages() {
    local npm_packages_array=("$@")
    local found_packages=()
    
    # Helper function to check if package was already found
    package_already_found() {
        local pkg="$1"
        local found
        for found in "${found_packages[@]}"; do
            [[ "$found" == "$pkg" ]] && return 0
        done
        return 1
    }

    # Check current npm root first
    local npm_root
    if npm_root=$(timeout 3 npm root -g 2>/dev/null) && [[ -n "$npm_root" && -d "$npm_root" ]]; then
        for package in "${npm_packages_array[@]}"; do
            local link_path="$npm_root/$package"
            if [[ -L "$link_path" ]] && ! package_already_found "$package"; then
                local target_path
                if target_path=$(readlink -f "$link_path" 2>/dev/null) && [[ -n "$target_path" ]]; then
                    echo "$package:$target_path"
                    found_packages+=("$package")
                fi
            fi
        done
    fi

    # Also check nvm directories for different Node versions
    if [[ -d "$HOME/.nvm/versions/node" ]]; then
        for node_version in "$HOME/.nvm/versions/node/"*; do
            if [[ -d "$node_version/lib/node_modules" ]]; then
                for package in "${npm_packages_array[@]}"; do
                    # Skip if already found
                    if package_already_found "$package"; then
                        continue
                    fi

                    local link_path="$node_version/lib/node_modules/$package"
                    if [[ -L "$link_path" ]]; then
                        local target_path
                        if target_path=$(readlink -f "$link_path" 2>/dev/null) && [[ -n "$target_path" ]]; then
                            echo "$package:$target_path"
                            found_packages+=("$package")
                        fi
                    fi
                done
            fi
        done
    fi
}

# Detect pip editable packages
detect_pip_packages() {
    local pip_packages_array=("$@")
    
    # Try multiple pip commands and environments
    for pip_cmd in pip pip3 python3 python; do
        if command -v $pip_cmd >/dev/null 2>&1; then
            local pip_output
            # Try different approaches for editable packages
            if pip_output=$(timeout 5 $pip_cmd -m pip list -e --format=json 2>/dev/null) || \
               pip_output=$(timeout 5 $pip_cmd list -e --format=json 2>/dev/null); then
                
                # Parse JSON output for editable packages
                if command -v jq >/dev/null 2>&1; then
                    echo "$pip_output" | jq -r '.[]? | select(.editable_project_location) | "\(.name):\(.editable_project_location)"' 2>/dev/null | while IFS=: read -r name location; do
                        if [[ -n "$name" && -n "$location" && -d "$location" ]]; then
                            # Check if this package matches requested packages (case-insensitive, handle dashes/underscores)
                            for requested_pkg in "${pip_packages_array[@]}"; do
                                if [[ "${name,,}" == "${requested_pkg,,}" ]] || \
                                   [[ "${name,,}" == "${requested_pkg,,/-/_}" ]] || \
                                   [[ "${name,,/-/_}" == "${requested_pkg,,}" ]] || \
                                   [[ "${name,,/-/_}" == "${requested_pkg,,/-/_}" ]]; then
                                    echo "$name:$location"
                                    break
                                fi
                            done
                        fi
                    done
                fi
                break  # Exit loop if we found a working pip command
            fi
        fi
    done
}

# Detect cargo linked packages
detect_cargo_packages() {
    local cargo_packages_array=("$@")
    
    # Check cargo install --list for path-based installs
    if command -v cargo >/dev/null 2>&1; then
        local cargo_output
        if cargo_output=$(timeout 3 cargo install --list 2>/dev/null); then
            # Parse cargo install list output for path-based installs
            echo "$cargo_output" | awk '
                /^[a-zA-Z0-9_-]+ .* \(.*\):$/ {
                    # Extract package name (first word)
                    pkg_name = $1
                    # Extract path from parentheses
                    if (match($0, /\(([^)]+)\):$/, arr)) {
                        pkg_path = arr[1]
                        if (index(pkg_path, "/") > 0) {
                            print pkg_name ":" pkg_path
                        }
                    }
                }
            ' | while IFS=: read -r pkg_name pkg_path; do
                if [[ -n "$pkg_name" && -n "$pkg_path" ]]; then
                    # Check if this package is in our requested list
                    for requested_pkg in "${cargo_packages_array[@]}"; do
                        if [[ "$pkg_name" == "$requested_pkg" ]]; then
                            echo "$pkg_name:$pkg_path"
                            break
                        fi
                    done
                fi
            done
        fi
    fi
}

# Main detection function with parallel execution
detect_linked_packages() {
    local package_manager="$1"
    shift
    local packages_array=("$@")
    
    # Validate package manager
    if ! validate_package_manager "$package_manager"; then
        return 1
    fi
    
    # Execute detection function for the specified package manager
    case "$package_manager" in
        "npm")
            detect_npm_packages "${packages_array[@]}"
            ;;
        "pip")
            detect_pip_packages "${packages_array[@]}"
            ;;
        "cargo")
            detect_cargo_packages "${packages_array[@]}"
            ;;
        *)
            echo "❌ Error: Unknown package manager '$package_manager'" >&2
            return 1
            ;;
    esac
}

# Generate volume mount strings for detected packages
generate_package_mounts() {
    local package_manager="$1"
    shift
    local packages_array=("$@")
    
    # Detect linked packages
    local detection_output
    if detection_output=$(detect_linked_packages "$package_manager" "${packages_array[@]}"); then
        # Convert detected packages to volume mount strings
        while IFS=: read -r package_name package_path; do
            if [[ -n "$package_name" && -n "$package_path" ]]; then
                # Use hierarchical mount structure: /workspace/.links/pm/package
                echo "$package_path:/workspace/.links/$package_manager/$package_name:delegated"
                echo "📦 Found linked package ($package_manager): $package_name -> $package_path" >&2
            fi
        done <<< "$detection_output"
    fi
}