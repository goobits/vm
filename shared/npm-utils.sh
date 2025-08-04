#!/bin/bash
# NPM utility functions for VM configuration
# Provides utilities for detecting and handling npm linked packages

set -e

# Detect npm linked packages and generate volume mounts
# Args: npm_packages_array - array of package names to search for
# Returns: Array of volume mount strings in format "source:destination:options"
# Output: Prints found package information to stderr for user feedback
detect_npm_linked_packages() {
	local npm_packages_array=("$@")
	local additional_volumes=()
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
	npm_root=$(npm root -g 2>/dev/null)
	if [[ -n "$npm_root" && -d "$npm_root" ]]; then
		for package in "${npm_packages_array[@]}"; do
			local link_path="$npm_root/$package"
			if [[ -L "$link_path" ]] && ! package_already_found "$package"; then
				local target_path
				target_path=$(readlink -f "$link_path")
				additional_volumes+=("$target_path:/workspace/.npm_links/$package:delegated")
				found_packages+=("$package")
				echo "📦 Found linked package: $package -> $target_path" >&2
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
						target_path=$(readlink -f "$link_path")
						additional_volumes+=("$target_path:/workspace/.npm_links/$package:delegated")
						found_packages+=("$package")
						echo "📦 Found linked package: $package -> $target_path (Node $(basename "$node_version"))" >&2
					fi
				done
			fi
		done
	fi

	printf '%s\n' "${additional_volumes[@]}"
}