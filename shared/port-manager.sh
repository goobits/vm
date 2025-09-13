#!/bin/bash
# Port Range Management for VM Projects
# Handles port range registration and conflict detection

set -e
set -u

# Port registry configuration
REGISTRY_DIR="$HOME/.vm"
REGISTRY_FILE="$REGISTRY_DIR/port-registry.json"

# Initialize the port registry if it doesn't exist
init_port_registry() {
    if [[ ! -d "$REGISTRY_DIR" ]]; then
        mkdir -p "$REGISTRY_DIR"
    fi
    
    if [[ ! -f "$REGISTRY_FILE" ]]; then
        echo '{}' > "$REGISTRY_FILE"
    fi
}

# Parse a range string (e.g., "3170-3179") into start and end
parse_range() {
    local range="$1"
    
    # Use simpler parsing that works in both bash and zsh
    if ! echo "$range" | grep -qE '^[0-9]+-[0-9]+$'; then
        echo "❌ Invalid port range format: $range" >&2
        echo "💡 Expected format: START-END (e.g., 3170-3179)" >&2
        return 1
    fi
    
    local start=$(echo "$range" | cut -d'-' -f1)
    local end=$(echo "$range" | cut -d'-' -f2)
    
    if [[ $start -ge $end ]]; then
        echo "❌ Invalid range: start ($start) must be less than end ($end)" >&2
        return 1
    fi
    
    echo "$start $end"
}

# Check if two ranges overlap
ranges_overlap() {
    local start1=$1
    local end1=$2
    local start2=$3
    local end2=$4
    
    # Ranges overlap if one starts before the other ends
    if [[ $start1 -le $end2 && $start2 -le $end1 ]]; then
        return 0  # Overlap detected
    fi
    
    return 1  # No overlap
}

# Check for port range conflicts
check_port_conflicts() {
    local range="$1"
    local project_name="${2:-}"
    
    init_port_registry
    
    # Parse the range
    local range_parts
    if ! range_parts=$(parse_range "$range"); then
        return 1
    fi
    
    local start end
    read -r start end <<< "$range_parts"
    
    # Check against all registered projects
    local conflicts=""
    
    if [[ -f "$REGISTRY_FILE" ]]; then
        # Get all projects from registry
        local projects
        projects=$(grep -o '"[^"]*"[[:space:]]*:' "$REGISTRY_FILE" 2>/dev/null | sed 's/^"\(.*\)"[[:space:]]*:.*/\1/' || echo "")
        
        for other_project in $projects; do
            # Skip checking against self
            if [[ "$other_project" == "$project_name" ]]; then
                continue
            fi
            
            # Get the other project's range
            local other_range
            other_range=$(grep -A 10 "\"$other_project\"" "$REGISTRY_FILE" 2>/dev/null | grep '"range"' | sed 's/.*"range"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/' || echo "")
            
            if [[ -n "$other_range" ]]; then
                local other_parts
                if other_parts=$(parse_range "$other_range" 2>/dev/null); then
                    local other_start other_end
                    read -r other_start other_end <<< "$other_parts"
                    
                    if ranges_overlap "$start" "$end" "$other_start" "$other_end"; then
                        if [[ -n "$conflicts" ]]; then
                            conflicts+=", "
                        fi
                        conflicts+="$other_project ($other_range)"
                    fi
                fi
            fi
        done
    fi
    
    if [[ -n "$conflicts" ]]; then
        echo "$conflicts"
        return 1
    fi
    
    return 0
}

# Register a port range for a project
register_port_range() {
    local project_name="$1"
    local range="$2"
    local project_path="$3"
    
    init_port_registry
    
    # Validate the range format
    if ! parse_range "$range" >/dev/null; then
        return 1
    fi
    
    # Check for conflicts
    local conflicts
    if conflicts=$(check_port_conflicts "$range" "$project_name"); then
        # No conflicts, safe to register
        local temp_file
        temp_file=$(mktemp)
        
        # Update the registry using sed (safe JSON manipulation for simple case)
        if [[ -s "$REGISTRY_FILE" ]] && ! grep -q '^[[:space:]]*{[[:space:]]*}[[:space:]]*$' "$REGISTRY_FILE"; then
            # File has existing content, add comma and new entry
            sed '$s/}$//' "$REGISTRY_FILE" > "$temp_file"
            echo "  ,\"$project_name\": {\"range\": \"$range\", \"path\": \"$project_path\"}" >> "$temp_file"
            echo "}" >> "$temp_file"
        else
            # Empty or minimal file, create new structure
            cat > "$temp_file" <<EOF
{
  "$project_name": {"range": "$range", "path": "$project_path"}
}
EOF
        fi
        
        mv "$temp_file" "$REGISTRY_FILE"
        
        echo "✅ Registered port range $range for project '$project_name'"
        return 0
    else
        echo "⚠️  Port range $range conflicts with: $conflicts"
        return 1
    fi
}

# Unregister a project's port range
unregister_port_range() {
    local project_name="$1"
    
    init_port_registry
    
    if [[ -f "$REGISTRY_FILE" ]]; then
        local temp_file
        temp_file=$(mktemp)
        
        # Remove the project from registry using sed
        # Remove the project entry and handle comma cleanup
        sed "/\"$project_name\"[[:space:]]*:[[:space:]]*{[^}]*}/d" "$REGISTRY_FILE" | \
        sed 's/,[[:space:]]*,/,/g; s/{[[:space:]]*,/{/; s/,[[:space:]]*}$/}/' > "$temp_file"
        mv "$temp_file" "$REGISTRY_FILE"
        
        echo "✅ Unregistered port range for project '$project_name'"
    fi
}

# Get a project's registered port range
get_project_range() {
    local project_name="$1"
    
    init_port_registry
    
    if [[ -f "$REGISTRY_FILE" ]]; then
        grep -A 10 "\"$project_name\"" "$REGISTRY_FILE" 2>/dev/null | grep '"range"' | sed 's/.*"range"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/' || echo ""
    fi
}

# List all registered port ranges
list_port_ranges() {
    init_port_registry
    
    if [[ -f "$REGISTRY_FILE" ]]; then
        echo "📡 Registered port ranges:"
        echo ""
        
        grep -o '"[^"]*"[[:space:]]*:[[:space:]]*{[^}]*}' "$REGISTRY_FILE" 2>/dev/null | while read -r entry; do
            project=$(echo "$entry" | sed 's/^"\([^"]*\)".*/\1/')
            range=$(echo "$entry" | sed 's/.*"range"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/')
            path=$(echo "$entry" | sed 's/.*"path"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/')
            echo "  $project: $range → $path"
        done || echo "  (none)"
    else
        echo "📡 No port ranges registered yet"
    fi
}

# Find next available port range
suggest_next_range() {
    local size="${1:-10}"
    local start_from="${2:-3000}"
    
    init_port_registry
    
    # Collect all used ranges
    local used_ranges=""
    if [[ -f "$REGISTRY_FILE" ]]; then
        used_ranges=$(grep '"range"' "$REGISTRY_FILE" 2>/dev/null | sed 's/.*"range"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/' | sort -n)
    fi
    
    # Find first available range
    local current=$start_from
    local found=false
    
    while [[ $current -lt 65535 ]]; do
        local end=$((current + size - 1))
        local candidate_range="$current-$end"
        
        # Check if this range conflicts
        if check_port_conflicts "$candidate_range" >/dev/null 2>&1; then
            echo "$candidate_range"
            found=true
            break
        fi
        
        # Try next range
        current=$((current + size))
    done
    
    if [[ "$found" == "false" ]]; then
        echo "❌ No available port range of size $size found" >&2
        return 1
    fi
}

# Functions are available when this script is sourced
# No need to export them explicitly