#!/bin/bash
#
# Version Bumper Script
#
# Automatically increments the package.json patch version (x.y.z -> x.y.z+1)
# and synchronizes the Rust workspace version from that release source.
# Usage: ./scripts/dev/bump-version.sh

set -euo pipefail

PACKAGE_JSON="package.json"
CARGO_TOML="rust/Cargo.toml"
CARGO_LOCK="rust/Cargo.lock"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PACKAGE_JSON_PATH="$PROJECT_ROOT/$PACKAGE_JSON"
CARGO_TOML_PATH="$PROJECT_ROOT/$CARGO_TOML"
CARGO_LOCK_PATH="$PROJECT_ROOT/$CARGO_LOCK"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if the release manifests exist
if [[ ! -f "$PACKAGE_JSON_PATH" || ! -f "$CARGO_TOML_PATH" || ! -f "$CARGO_LOCK_PATH" ]]; then
    echo -e "${RED}❌ Error: $PACKAGE_JSON, $CARGO_TOML, and $CARGO_LOCK are required${NC}"
    exit 1
fi

# Extract the current version from the release source of truth.
CURRENT_VERSION=$(sed -n 's/^[[:space:]]*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$PACKAGE_JSON_PATH" | head -1)

if [[ -z "$CURRENT_VERSION" ]]; then
    echo -e "${RED}❌ Error: Could not extract version from $PACKAGE_JSON${NC}"
    exit 1
fi

echo -e "${BLUE}📌 Current version: $CURRENT_VERSION${NC}"

# Parse version components
if [[ ! "$CURRENT_VERSION" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
    echo -e "${RED}❌ Error: Invalid version format '$CURRENT_VERSION' (expected: x.y.z)${NC}"
    exit 1
fi

MAJOR="${BASH_REMATCH[1]}"
MINOR="${BASH_REMATCH[2]}"
PATCH="${BASH_REMATCH[3]}"

# Increment patch version
NEW_PATCH=$((PATCH + 1))
NEW_VERSION="$MAJOR.$MINOR.$NEW_PATCH"

echo -e "${GREEN}🚀 Bumping version: $CURRENT_VERSION → $NEW_VERSION${NC}"

# Create backups so a failed sync can restore every version-owned file.
PACKAGE_BACKUP=$(mktemp)
CARGO_BACKUP=$(mktemp)
CARGO_LOCK_BACKUP=$(mktemp)
cp "$PACKAGE_JSON_PATH" "$PACKAGE_BACKUP"
cp "$CARGO_TOML_PATH" "$CARGO_BACKUP"
cp "$CARGO_LOCK_PATH" "$CARGO_LOCK_BACKUP"
RESTORE_ON_EXIT=true

cleanup_backups() {
    rm -f "$PACKAGE_BACKUP" "$CARGO_BACKUP" "$CARGO_LOCK_BACKUP"
}

restore_version_files() {
    cp "$PACKAGE_BACKUP" "$PACKAGE_JSON_PATH"
    cp "$CARGO_BACKUP" "$CARGO_TOML_PATH"
    cp "$CARGO_LOCK_BACKUP" "$CARGO_LOCK_PATH"
    cleanup_backups
}

trap 'if [[ "$RESTORE_ON_EXIT" == true ]]; then restore_version_files; fi' EXIT

# Update the root release version.
if [[ "$OSTYPE" == "darwin"* ]]; then
    sed -i '' "s/\"version\": \"$CURRENT_VERSION\"/\"version\": \"$NEW_VERSION\"/" "$PACKAGE_JSON_PATH"
else
    sed -i "s/\"version\": \"$CURRENT_VERSION\"/\"version\": \"$NEW_VERSION\"/" "$PACKAGE_JSON_PATH"
fi

# Verify the change
NEW_VERSION_CHECK=$(sed -n 's/^[[:space:]]*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$PACKAGE_JSON_PATH" | head -1)

if [[ "$NEW_VERSION_CHECK" == "$NEW_VERSION" ]]; then
    echo -e "${BLUE}🔄 Synchronizing $CARGO_TOML from $PACKAGE_JSON...${NC}"
    cd "$PROJECT_ROOT"
    if ! cargo run --manifest-path "$CARGO_TOML_PATH" --package version-sync --quiet -- sync; then
        echo -e "${RED}❌ Error: Version synchronization failed${NC}"
        exit 1
    fi

    # Update Cargo.lock
    echo -e "${BLUE}📦 Updating Cargo.lock...${NC}"
    if ! cargo check --manifest-path "$CARGO_TOML_PATH" --workspace --quiet; then
        echo -e "${RED}❌ Error: Cargo.lock update failed${NC}"
        exit 1
    fi

    RESTORE_ON_EXIT=false
    cleanup_backups
    echo -e "${GREEN}✅ Version successfully updated to $NEW_VERSION${NC}"

    echo -e "${GREEN}✨ Version bump complete!${NC}"
    echo ""
    echo -e "${YELLOW}Don't forget to commit the changes:${NC}"
    echo -e "  git add package.json rust/Cargo.toml rust/Cargo.lock"
    echo -e "  git commit -m \"chore: bump version to $NEW_VERSION\""
else
    echo -e "${RED}❌ Error: Version update verification failed${NC}"
    echo -e "${YELLOW}Restoring backup...${NC}"
    exit 1
fi
