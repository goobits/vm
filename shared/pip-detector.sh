#!/bin/bash
# Comprehensive pip/pipx package detection for VM configuration
# Detects pipx installations, regular pip packages, and editable packages
# Returns package:path pairs for volume mounting in containers

set -e
set -u

# Get script directory for relative imports
PIP_DETECTOR_SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Helper function to normalize package names (handle dashes/underscores)
normalize_package_name() {
    local name="$1"
    echo "$name" | tr '[:upper:]' '[:lower:]' | tr '-' '_'
}

# Check if a package name matches any in the requested list (case-insensitive, dash/underscore flexible)
package_matches_request() {
    local package_name="$1"
    shift
    local requested_packages=("$@")

    local pkg_lower="$(echo "$package_name" | tr '[:upper:]' '[:lower:]')"
    local pkg_normalized="${pkg_lower//-/_}"

    for requested_pkg in "${requested_packages[@]}"; do
        local req_lower="$(echo "$requested_pkg" | tr '[:upper:]' '[:lower:]')"
        local req_normalized="${req_lower//-/_}"

        if [[ "$pkg_lower" == "$req_lower" ]] || \
           [[ "$pkg_lower" == "$req_normalized" ]] || \
           [[ "$pkg_normalized" == "$req_lower" ]] || \
           [[ "$pkg_normalized" == "$req_normalized" ]]; then
            return 0
        fi
    done
    return 1
}

# Detect pipx packages installed in isolated environments
detect_pipx_packages() {
    local pip_packages_array=("$@")
    local pipx_home=""

    # Try different possible pipx venv locations
    local possible_paths=(
        "$HOME/.local/share/pipx/venvs"  # Linux/standard
        "$HOME/.local/pipx/venvs"        # macOS/alternative
        "${PIPX_HOME:-}/venvs"           # Custom PIPX_HOME
    )

    for path in "${possible_paths[@]}"; do
        if [[ -n "$path" && -d "$path" ]]; then
            pipx_home="$path"
            break
        fi
    done

    if [[ -z "$pipx_home" ]]; then
        return 0
    fi

    # Check each pipx virtual environment
    for venv_dir in "$pipx_home"/*; do
        if [[ -d "$venv_dir" ]]; then
            local package_name="$(basename "$venv_dir")"

            # Check if this package was requested
            if package_matches_request "$package_name" "${pip_packages_array[@]}"; then
                # Find the actual package location in the venv
                local site_packages=""
                for python_dir in "$venv_dir/lib"/python*; do
                    if [[ -d "$python_dir/site-packages" ]]; then
                        site_packages="$python_dir/site-packages"
                        break
                    fi
                done

                if [[ -n "$site_packages" ]]; then
                    # Look for the actual package directory
                    local package_found=false
                    for item in "$site_packages"/*; do
                        if [[ -d "$item" && ! "$(basename "$item")" =~ .*\.dist-info$ && ! "$(basename "$item")" =~ .*\.egg-info$ ]]; then
                            local item_name="$(basename "$item")"
                            if package_matches_request "$item_name" "${pip_packages_array[@]}"; then
                                echo "$package_name:$item"
                                package_found=true
                                break
                            fi
                        fi
                    done

                    # If no exact package directory found, use the venv directory itself
                    if [[ "$package_found" == false ]]; then
                        echo "$package_name:$venv_dir"
                    fi
                fi
            fi
        fi
    done
}

# Detect regular pip packages using pip show
detect_regular_pip_packages() {
    local pip_packages_array=("$@")
    local found_packages=()

    # Try different pip commands
    for pip_cmd in pip pip3 python3 python; do
        if command -v "$pip_cmd" >/dev/null 2>&1; then
            for package in "${pip_packages_array[@]}"; do
                # Skip if already found
                local already_found=false
                for found_pkg in "${found_packages[@]:-}"; do
                    if [[ "$found_pkg" == "$package" ]]; then
                        already_found=true
                        break
                    fi
                done
                [[ "$already_found" == true ]] && continue

                # Use pip show to get package location
                local pip_show_output
                case "$pip_cmd" in
                    pip|pip3)
                        pip_show_output=$($pip_cmd show "$package" 2>/dev/null || true)
                        ;;
                    python|python3)
                        pip_show_output=$($pip_cmd -m pip show "$package" 2>/dev/null || true)
                        ;;
                esac

                if [[ -n "$pip_show_output" ]]; then
                    local location
                    location=$(echo "$pip_show_output" | grep "^Location:" | cut -d' ' -f2- | tr -d '[:space:]')

                    if [[ -n "$location" && -d "$location" ]]; then
                        # Check if this is an editable install (not in site-packages)
                        if [[ "$location" != *"site-packages"* ]]; then
                            # Editable install - use location directly if it exists
                            if [[ -d "$location" ]]; then
                                echo "$package:$location"
                                found_packages+=("$package")
                            fi
                        else
                            # Regular install - find the package directory in site-packages
                            local package_dir=""
                            for variant in "$package" "$(echo "$package" | tr '-' '_')" "$(echo "$package" | tr '_' '-')"; do
                                if [[ -d "$location/$variant" ]]; then
                                    package_dir="$location/$variant"
                                    break
                                fi
                            done

                            if [[ -n "$package_dir" ]]; then
                                echo "$package:$package_dir"
                                found_packages+=("$package")
                            else
                                # Fallback to site-packages directory itself
                                echo "$package:$location"
                                found_packages+=("$package")
                            fi
                        fi
                    fi
                fi
            done
            break  # Exit loop after first working pip command
        fi
    done
}

# Detect editable pip packages (for development)
detect_editable_pip_packages() {
    local pip_packages_array=("$@")

    # Try different pip commands and environments
    for pip_cmd in pip pip3 python3 python; do
        if command -v "$pip_cmd" >/dev/null 2>&1; then
            local pip_output
            # Try different approaches for editable packages based on command type
            case "$pip_cmd" in
                pip|pip3)
                    # Direct pip commands don't use -m flag
                    pip_output=$($pip_cmd list -e --format=json 2>/dev/null || true)
                    ;;
                python|python3)
                    # Python commands need -m pip
                    pip_output=$($pip_cmd -m pip list -e --format=json 2>/dev/null || true)
                    ;;
            esac

            if [[ -n "$pip_output" ]]; then
                # Parse JSON output for editable packages
                if command -v jq >/dev/null 2>&1; then
                    echo "$pip_output" | jq -r '.[]? | select(.editable_project_location) | "\(.name):\(.editable_project_location)"' 2>/dev/null | while IFS=: read -r name location; do
                        if [[ -n "$name" && -n "$location" && -d "$location" ]]; then
                            # Check if this package matches requested packages
                            if package_matches_request "$name" "${pip_packages_array[@]}"; then
                                echo "$name:$location"
                            fi
                        fi
                    done
                fi
                break  # Exit loop if we found a working pip command
            fi
        fi
    done
}

# Helper function to check if package was already found (bash 3.x compatible)
package_already_found() {
    local package_name="$1"
    local found_list="$2"

    # Check if package name appears in space-separated list
    case " $found_list " in
        *" $package_name "*) return 0 ;;
        *) return 1 ;;
    esac
}

# Main detection function - combines all detection methods
detect_all_pip_packages() {
    local pip_packages_array=("$@")
    local found_packages=""  # Space-separated list for bash 3.x compatibility

    # 1. First try pipx packages (highest priority for CLI tools)
    while IFS=: read -r package_name package_path; do
        if [[ -n "$package_name" && -n "$package_path" ]]; then
            if ! package_already_found "$package_name" "$found_packages"; then
                echo "$package_name:$package_path"
                found_packages="$found_packages $package_name"
            fi
        fi
    done < <(detect_pipx_packages "${pip_packages_array[@]}")

    # 2. Then try editable packages (second priority for development)
    while IFS=: read -r package_name package_path; do
        if [[ -n "$package_name" && -n "$package_path" ]]; then
            if ! package_already_found "$package_name" "$found_packages"; then
                echo "$package_name:$package_path"
                found_packages="$found_packages $package_name"
            fi
        fi
    done < <(detect_editable_pip_packages "${pip_packages_array[@]}")

    # 3. Finally try regular pip packages (lowest priority)
    while IFS=: read -r package_name package_path; do
        if [[ -n "$package_name" && -n "$package_path" ]]; then
            if ! package_already_found "$package_name" "$found_packages"; then
                echo "$package_name:$package_path"
                found_packages="$found_packages $package_name"
            fi
        fi
    done < <(detect_regular_pip_packages "${pip_packages_array[@]}")
}

# Generate volume mount strings for Docker/container use
generate_pip_volume_mounts() {
    local pip_packages_array=("$@")

    # Detect all pip packages
    while IFS=: read -r package_name package_path; do
        if [[ -n "$package_name" && -n "$package_path" ]]; then
            # Use hierarchical mount structure: /home/developer/.links/pip/package
            echo "$package_path:/home/developer/.links/pip/$package_name:delegated"
            echo "📦 Found linked package (pip): $package_name -> $package_path" >&2
        fi
    done < <(detect_all_pip_packages "${pip_packages_array[@]}")
}

# Export functions for use by other scripts
if [[ "${BASH_SOURCE[0]}" != "${0}" ]]; then
    # Script is being sourced - export functions
    export -f detect_pipx_packages
    export -f detect_regular_pip_packages
    export -f detect_editable_pip_packages
    export -f detect_all_pip_packages
    export -f generate_pip_volume_mounts
    export -f package_matches_request
    export -f normalize_package_name
fi

# Allow direct execution for testing
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    if [[ $# -eq 0 ]]; then
        echo "Usage: $0 <package1> [package2] [package3] ..." >&2
        echo "Example: $0 requests httpie black" >&2
        exit 1
    fi

    detect_all_pip_packages "$@"
fi