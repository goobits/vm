#!/bin/bash
#
# Build script for custom base images
# Usage: ./build.sh [image-name]
#
# Examples:
#   ./build.sh playwright      # Build playwright-chromium image
#   ./build.sh fullstack       # Build full-stack-dev image
#   ./build.sh minimal         # Build minimal-node image
#   ./build.sh all             # Build all images

set -e  # Exit on error

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Function to build an image
build_image() {
    local dockerfile=$1
    local tag=$2
    local description=$3

    echo -e "${BLUE}Building ${description}...${NC}"
    echo -e "${YELLOW}Dockerfile: ${dockerfile}${NC}"
    echo -e "${YELLOW}Tag: ${tag}${NC}"
    echo ""

    if docker build -f "$dockerfile" -t "$tag" .; then
        echo -e "${GREEN}✓ Successfully built ${tag}${NC}"
        echo ""
        return 0
    else
        echo -e "${RED}✗ Failed to build ${tag}${NC}"
        echo ""
        return 1
    fi
}

# Function to show usage
show_usage() {
    echo "Usage: $0 [image-name]"
    echo ""
    echo "Available images:"
    echo "  supercool    - Supercool (Node + Bun + Rust + Python + Playwright) ~2GB"
    echo "  minimal      - Minimal Node.js (Just Node + essentials) ~500MB"
    echo "  all          - Build both images"
    echo ""
    echo "Examples:"
    echo "  $0 supercool    # Recommended for most projects"
    echo "  $0 minimal      # For lightweight projects"
    echo "  $0 all"
}

# Main script
case "${1:-}" in
    supercool)
        build_image "supercool.dockerfile" \
                   "supercool:latest" \
                   "Supercool (Ultimate Dev Environment)"
        ;;

    minimal)
        build_image "minimal-node.dockerfile" \
                   "vm-minimal-base:latest" \
                   "Minimal Node.js Base"
        ;;

    all)
        echo -e "${BLUE}Building all base images...${NC}"
        echo ""

        build_image "minimal-node.dockerfile" \
                   "vm-minimal-base:latest" \
                   "Minimal Node.js Base"

        build_image "supercool.dockerfile" \
                   "supercool:latest" \
                   "Supercool (Ultimate)"

        echo -e "${GREEN}✓ All images built successfully!${NC}"
        echo ""
        echo "Available images:"
        docker images | grep -E "vm-minimal-base|supercool" || true
        ;;

    "")
        show_usage
        exit 0
        ;;

    *)
        echo -e "${RED}Unknown image: $1${NC}"
        echo ""
        show_usage
        exit 1
        ;;
esac

# Show built image info
if [ "$1" != "all" ] && [ "$1" != "" ]; then
    echo -e "${BLUE}Image details:${NC}"
    docker images | grep "${1}" || docker images | grep "vm-.*-base" | head -1
    echo ""
    echo -e "${GREEN}To use this image in your project:${NC}"
    echo ""
    echo "vm:"
    echo "  box_name: $(docker images --format "{{.Repository}}:{{.Tag}}" | grep "${1}" | head -1 || echo "vm-${1}-base:latest")"
fi
