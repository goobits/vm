#!/bin/bash
# Build script for optimized Vibe Dockerfile
# Provides timing information and build options

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output
print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if BuildKit is available
check_buildkit() {
    print_info "Checking Docker version and BuildKit support..."

    DOCKER_VERSION=$(docker --version | grep -oP '\d+\.\d+' | head -1)
    MAJOR=$(echo $DOCKER_VERSION | cut -d. -f1)
    MINOR=$(echo $DOCKER_VERSION | cut -d. -f2)

    if [ "$MAJOR" -lt 18 ] || ([ "$MAJOR" -eq 18 ] && [ "$MINOR" -lt 9 ]); then
        print_error "Docker version $DOCKER_VERSION detected. BuildKit requires Docker 18.09+"
        exit 1
    fi

    print_success "Docker version $DOCKER_VERSION supports BuildKit"
}

# Display usage
usage() {
    cat << EOF
Usage: $0 [OPTIONS]

Build the optimized Vibe development environment Docker image.

OPTIONS:
    -h, --help          Show this help message
    -t, --tag TAG       Tag for the image (default: vibe-box-optimized)
    -f, --file FILE     Dockerfile to use (default: Dockerfile.vibe.optimized)
    -c, --compare       Build both original and optimized for comparison
    -p, --prune         Prune build cache before building (cold build)
    --progress TYPE     Progress output type: auto, plain, tty (default: auto)
    --no-cache          Build without using cache
    --target STAGE      Build only up to specified stage

EXAMPLES:
    # Standard optimized build
    $0

    # Build with custom tag
    $0 -t my-vibe-box:latest

    # Compare original vs optimized
    $0 --compare

    # Cold build (no cache)
    $0 --prune

    # Build with plain progress output
    $0 --progress plain

    # Build only node-builder stage
    $0 --target node-builder

EOF
}

# Default values
TAG="vibe-box-optimized"
DOCKERFILE="Dockerfile.vibe.optimized"
COMPARE=false
PRUNE=false
PROGRESS="auto"
NO_CACHE=""
TARGET=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            usage
            exit 0
            ;;
        -t|--tag)
            TAG="$2"
            shift 2
            ;;
        -f|--file)
            DOCKERFILE="$2"
            shift 2
            ;;
        -c|--compare)
            COMPARE=true
            shift
            ;;
        -p|--prune)
            PRUNE=true
            shift
            ;;
        --progress)
            PROGRESS="$2"
            shift 2
            ;;
        --no-cache)
            NO_CACHE="--no-cache"
            shift
            ;;
        --target)
            TARGET="--target $2"
            shift 2
            ;;
        *)
            print_error "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

# Check BuildKit support
check_buildkit

# Prune cache if requested
if [ "$PRUNE" = true ]; then
    print_warning "Pruning build cache for cold build test..."
    docker builder prune -a -f
    print_success "Build cache cleared"
fi

# Function to build an image
build_image() {
    local dockerfile=$1
    local tag=$2
    local build_name=$3

    print_info "Building $build_name..."
    print_info "Dockerfile: $dockerfile"
    print_info "Tag: $tag"

    # Record start time
    START_TIME=$(date +%s)

    # Build command
    DOCKER_BUILDKIT=1 docker build \
        --progress=$PROGRESS \
        $NO_CACHE \
        $TARGET \
        -t "$tag" \
        -f "$dockerfile" \
        .

    # Record end time
    END_TIME=$(date +%s)
    BUILD_TIME=$((END_TIME - START_TIME))

    print_success "$build_name completed in ${BUILD_TIME}s"
    echo ""

    return $BUILD_TIME
}

# Compare mode: build both original and optimized
if [ "$COMPARE" = true ]; then
    print_info "=== COMPARISON MODE ==="
    print_info "Building both original and optimized Dockerfiles"
    echo ""

    # Check if original exists
    if [ ! -f "Dockerfile.vibe" ]; then
        print_error "Original Dockerfile.vibe not found!"
        exit 1
    fi

    # Check if optimized exists
    if [ ! -f "Dockerfile.vibe.optimized" ]; then
        print_error "Optimized Dockerfile.vibe.optimized not found!"
        exit 1
    fi

    # Build original
    print_info "Step 1/2: Building original Dockerfile..."
    build_image "Dockerfile.vibe" "vibe-box-original" "Original build"
    ORIGINAL_TIME=$?

    # Build optimized
    print_info "Step 2/2: Building optimized Dockerfile..."
    build_image "Dockerfile.vibe.optimized" "vibe-box-optimized" "Optimized build"
    OPTIMIZED_TIME=$?

    # Calculate improvement
    IMPROVEMENT=$((ORIGINAL_TIME - OPTIMIZED_TIME))
    PERCENT=$(( (IMPROVEMENT * 100) / ORIGINAL_TIME ))

    # Print comparison
    echo ""
    print_info "=== BUILD TIME COMPARISON ==="
    echo "Original build:  ${ORIGINAL_TIME}s"
    echo "Optimized build: ${OPTIMIZED_TIME}s"
    echo ""

    if [ $IMPROVEMENT -gt 0 ]; then
        print_success "Time saved: ${IMPROVEMENT}s (${PERCENT}% faster)"
    elif [ $IMPROVEMENT -lt 0 ]; then
        print_warning "Optimized build was slower by ${IMPROVEMENT#-}s"
    else
        print_info "Build times are identical"
    fi

    echo ""
    print_info "Image sizes:"
    docker images | grep -E "REPOSITORY|vibe-box"

else
    # Single build mode
    if [ ! -f "$DOCKERFILE" ]; then
        print_error "Dockerfile not found: $DOCKERFILE"
        exit 1
    fi

    build_image "$DOCKERFILE" "$TAG" "Vibe development environment"

    # Show image info
    print_info "Image information:"
    docker images | grep -E "REPOSITORY|$TAG"

    echo ""
    print_success "Build complete! Run with:"
    echo "  docker run -it --rm $TAG /bin/bash"
fi

echo ""
print_info "To test the container, run the following commands inside it:"
cat << 'EOF'

  # Test Node.js
  node --version && npm --version

  # Test Python
  python --version && pip3 --version

  # Test Rust
  rustc --version && cargo --version

  # Test AI tools
  claude --version && gemini --version

  # Test Playwright
  npx playwright --version

EOF
