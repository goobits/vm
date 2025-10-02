#!/bin/bash
#
# Version Bumper Script
#
# Automatically increments the patch version (x.y.z -> x.y.z+1) in Cargo.toml
# Usage: ./scripts/bump-version.sh

set -euo pipefail

CARGO_TOML="rust/Cargo.toml"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CARGO_TOML_PATH="$PROJECT_ROOT/$CARGO_TOML"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if Cargo.toml exists
if [[ ! -f "$CARGO_TOML_PATH" ]]; then
    echo -e "${RED}❌ Error: $CARGO_TOML not found${NC}"
    exit 1
fi

# Extract current version from workspace Cargo.toml
CURRENT_VERSION=$(grep '^\s*version\s*=\s*"' "$CARGO_TOML_PATH" | head -1 | sed 's/.*version\s*=\s*"\([^"]*\)".*/\1/')

if [[ -z "$CURRENT_VERSION" ]]; then
    echo -e "${RED}❌ Error: Could not extract version from $CARGO_TOML${NC}"
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

# Create backup
BACKUP_FILE="$CARGO_TOML_PATH.backup"
cp "$CARGO_TOML_PATH" "$BACKUP_FILE"

# Update version in Cargo.toml
# Use a more robust sed command that only updates the workspace version
if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS sed requires different syntax
    sed -i '' "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" "$CARGO_TOML_PATH"
else
    # Linux sed
    sed -i "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" "$CARGO_TOML_PATH"
fi

# Verify the change
NEW_VERSION_CHECK=$(grep '^\s*version\s*=\s*"' "$CARGO_TOML_PATH" | head -1 | sed 's/.*version\s*=\s*"\([^"]*\)".*/\1/')

if [[ "$NEW_VERSION_CHECK" == "$NEW_VERSION" ]]; then
    echo -e "${GREEN}✅ Version successfully updated to $NEW_VERSION${NC}"
    rm -f "$BACKUP_FILE"

    # Update Cargo.lock
    echo -e "${BLUE}📦 Updating Cargo.lock...${NC}"
    cd "$PROJECT_ROOT/rust"
    cargo check --workspace --quiet 2>/dev/null || cargo check --workspace

    echo -e "${GREEN}✨ Version bump complete!${NC}"
    echo ""
    echo -e "${YELLOW}Don't forget to commit the changes:${NC}"
    echo -e "  git add rust/Cargo.toml rust/Cargo.lock"
    echo -e "  git commit -m \"chore: bump version to $NEW_VERSION\""
else
    echo -e "${RED}❌ Error: Version update verification failed${NC}"
    echo -e "${YELLOW}Restoring backup...${NC}"
    mv "$BACKUP_FILE" "$CARGO_TOML_PATH"
    exit 1
fi
