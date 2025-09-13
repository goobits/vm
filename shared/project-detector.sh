#!/bin/bash
# Project type detection module for VM tool smart presets
# Detects project type based on characteristic files and directories

# Function to detect the project type based on files in the current directory
# Returns: project type string (e.g., "nodejs", "python", "multi:nodejs python", "generic")
detect_project_type() {
    local detected_types=()
    local project_dir="${1:-.}"

    # Change to project directory for detection
    pushd "$project_dir" > /dev/null 2>&1 || return 1

    # Node.js detection with framework detection
    if [[ -f "package.json" ]]; then
        local framework="nodejs"
        # Parse package.json to detect frameworks
        if command -v jq >/dev/null 2>&1; then
            # Use jq if available for robust JSON parsing
            local deps
            deps=$(yq eval '.dependencies // {} | keys[]' package.json 2>/dev/null)
            local devdeps
            devdeps=$(yq eval '.devDependencies // {} | keys[]' package.json 2>/dev/null)
            local all_deps
            all_deps=$(echo -e "$deps\n$devdeps")

            # Check for more specific frameworks first
            if echo "$all_deps" | grep -q "^next$"; then
                framework="next"
            elif echo "$all_deps" | grep -q "^@angular/core$"; then
                framework="angular"
            elif echo "$all_deps" | grep -q "^vue$"; then
                framework="vue"
            elif echo "$all_deps" | grep -q "^react$"; then
                framework="react"
            fi
        else
            # Fallback: simple grep-based detection (less reliable but works without jq)
            # Check for more specific frameworks first
            if grep -q '"next"' package.json 2>/dev/null; then
                framework="next"
            elif grep -q '"@angular/core"' package.json 2>/dev/null; then
                framework="angular"
            elif grep -q '"vue"' package.json 2>/dev/null; then
                framework="vue"
            elif grep -q '"react"' package.json 2>/dev/null; then
                framework="react"
            fi
        fi
        detected_types+=("$framework")
    fi

    # Python detection with framework detection
    if [[ -f "requirements.txt" ]] || [[ -f "pyproject.toml" ]] || \
       [[ -f "setup.py" ]] || [[ -f "Pipfile" ]]; then
        local framework="python"

        # Helper function to detect Python framework from file
        detect_python_framework() {
            local file="$1"
            local pattern_prefix="$2"  # e.g., "^" for requirements.txt, "" for others

            if [[ -f "$file" ]]; then
                if grep -iq "${pattern_prefix}django" "$file" 2>/dev/null; then
                    echo "django"
                    return 0
                elif grep -iq "${pattern_prefix}flask" "$file" 2>/dev/null; then
                    echo "flask"
                    return 0
                fi
            fi
            echo "python"
        }

        # Check files in order of preference (requirements.txt first as most explicit)
        framework=$(detect_python_framework "requirements.txt" "^")
        [[ "$framework" == "python" ]] && framework=$(detect_python_framework "pyproject.toml" "")
        [[ "$framework" == "python" ]] && framework=$(detect_python_framework "Pipfile" "")

        detected_types+=("$framework")
    fi

    # Rust detection
    if [[ -f "Cargo.toml" ]]; then
        detected_types+=("rust")
    fi

    # Go detection
    if [[ -f "go.mod" ]]; then
        detected_types+=("go")
    fi

    # Ruby detection with Rails framework detection
    if [[ -f "Gemfile" ]]; then
        local framework="ruby"
        # Check for Rails in Gemfile
        if grep -iq "gem ['\"]rails['\"]" Gemfile 2>/dev/null; then
            framework="rails"
        fi
        detected_types+=("$framework")
    fi

    # PHP detection
    if [[ -f "composer.json" ]]; then
        detected_types+=("php")
    fi

    # Docker detection (additional, not a language but important for environment)
    if [[ -f "docker-compose.yml" ]] || [[ -f "docker-compose.yaml" ]] || \
       [[ -f "Dockerfile" ]]; then
        detected_types+=("docker")
    fi

    # Kubernetes detection
    if [[ -d "kubernetes" ]] || [[ -d "k8s" ]] || \
       [[ -f "k8s.yaml" ]] || [[ -f "k8s.yml" ]]; then
        detected_types+=("kubernetes")
    fi

    # Return to original directory
    popd > /dev/null 2>&1

    # Process results
    local type_count=${#detected_types[@]}

    if [[ $type_count -eq 0 ]]; then
        echo "generic"
    elif [[ $type_count -eq 1 ]]; then
        echo "${detected_types[0]}"
    else
        # Multiple types detected - return as multi:type1 type2 type3
        echo "multi:${detected_types[*]}"
    fi
}

# Function to check if project has version control
has_version_control() {
    local project_dir="${1:-.}"
    [[ -d "$project_dir/.git" ]]
}

# Function to get detailed project information
get_project_info() {
    local project_dir="${1:-.}"
    local project_type=$(detect_project_type "$project_dir")

    echo "Project Type: $project_type"

    if has_version_control "$project_dir"; then
        echo "Version Control: Git"
    else
        echo "Version Control: None"
    fi

    # Additional details based on project type
    case "$project_type" in
        nodejs|react|vue|next|angular)
            if [[ -f "$project_dir/package.json" ]]; then
                local pkg_manager="npm"
                [[ -f "$project_dir/yarn.lock" ]] && pkg_manager="yarn"
                [[ -f "$project_dir/pnpm-lock.yaml" ]] && pkg_manager="pnpm"
                echo "Package Manager: $pkg_manager"
                [[ "$project_type" != "nodejs" ]] && echo "Framework: $project_type"
            fi
            ;;
        python|django|flask)
            local py_tools=()
            [[ -f "$project_dir/requirements.txt" ]] && py_tools+=("pip")
            [[ -f "$project_dir/pyproject.toml" ]] && py_tools+=("poetry/setuptools")
            [[ -f "$project_dir/Pipfile" ]] && py_tools+=("pipenv")
            [[ ${#py_tools[@]} -gt 0 ]] && echo "Python Tools: ${py_tools[*]}"
            [[ "$project_type" != "python" ]] && echo "Framework: $project_type"
            ;;
        ruby|rails)
            [[ "$project_type" == "rails" ]] && echo "Framework: Rails"
            ;;
        multi:*)
            echo "Multi-language project detected"
            ;;
    esac
}

# Function to suggest VM resources based on project type
suggest_vm_resources() {
    local project_type="$1"

    case "$project_type" in
        nodejs|react|vue|next|angular|python|django|flask|ruby|rails|php)
            echo "memory=2048 cpus=2 disk_size=20"
            ;;
        rust|go)
            echo "memory=4096 cpus=4 disk_size=30"
            ;;
        docker|kubernetes)
            echo "memory=4096 cpus=4 disk_size=40"
            ;;
        multi:*)
            # For multi-language projects, suggest higher resources
            echo "memory=4096 cpus=4 disk_size=40"
            ;;
        generic)
            # Default resources
            echo "memory=2048 cpus=2 disk_size=20"
            ;;
        *)
            echo "memory=2048 cpus=2 disk_size=20"
            ;;
    esac
}

# Functions are available when this script is sourced
# No need to export when sourcing