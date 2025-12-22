#!/bin/bash
# Test WebGL/WebGPU support in Docker before rebuilding @vibe-box
# Usage: ./scripts/test-webgl-docker.sh [--no-cache] [--skip-build]
#   --no-cache   Force fresh build (ignore Docker cache)
#   --skip-build Reuse existing vibe-box-test image (for quick retests)

set -e

IMAGE_NAME="vibe-box-test"
CONTAINER_NAME="vibe-webgl-test"
BUILD_ARGS=""
SKIP_BUILD=false

# Parse arguments
for arg in "$@"; do
    case $arg in
        --no-cache)
            BUILD_ARGS="--no-cache"
            echo "Note: Using --no-cache, this will take 15-25 minutes"
            ;;
        --skip-build)
            SKIP_BUILD=true
            ;;
    esac
done

echo "=== WebGL/WebGPU Docker Test ==="
echo ""

# Build test image
if [ "$SKIP_BUILD" = true ]; then
    echo "[1/4] Skipping build, reusing existing image..."
    if ! docker image inspect "$IMAGE_NAME" >/dev/null 2>&1; then
        echo "ERROR: Image $IMAGE_NAME not found. Run without --skip-build first."
        exit 1
    fi
else
    echo "[1/4] Building test image from Dockerfile.vibe..."
    docker build $BUILD_ARGS -t "$IMAGE_NAME" -f Dockerfile.vibe .
fi

echo ""
echo "[2/4] Starting container..."
docker rm -f "$CONTAINER_NAME" 2>/dev/null || true
docker run -d --name "$CONTAINER_NAME" "$IMAGE_NAME" sleep 300

echo ""
echo "[3/4] Running WebGL/WebGPU tests..."

# Test 1: Check tini is running as PID 1
echo ""
echo "--- Test: tini init system ---"
docker exec "$CONTAINER_NAME" bash -c 'ps -p 1 -o comm=' | grep -q tini && echo "PASS: tini is PID 1" || echo "FAIL: tini is not PID 1"

# Test 2: Check Mesa/EGL libraries are installed
echo ""
echo "--- Test: Mesa/EGL libraries ---"
docker exec "$CONTAINER_NAME" bash -c '
    # Check for actual library files (search all lib paths including arch-specific)
    check_lib() {
        name="$1"
        pattern="$2"
        found=$(find /usr/lib* /lib* -name "$pattern" 2>/dev/null | head -1)
        if [ -n "$found" ]; then
            echo "PASS: $name found at $found"
            return 0
        else
            echo "FAIL: $name not found (searched for $pattern)"
            return 1
        fi
    }
    check_lib "libEGL" "libEGL.so*"
    check_lib "libGL" "libGL.so*"
    check_lib "libGLESv2" "libGLESv2.so*"
    check_lib "libvulkan" "libvulkan.so*"

    # Also show what EGL/GL related packages are installed
    echo ""
    echo "Installed EGL/GL packages:"
    dpkg -l | grep -iE "libegl|libgl|libgles|mesa" | awk "{print \"  \" \$2}" || echo "  (none found)"
'

# Test 3: Check Vulkan ICD file exists
echo ""
echo "--- Test: Vulkan ICD configuration ---"
docker exec "$CONTAINER_NAME" bash -c '
    ARCH=$(uname -m)
    ICD_FILE="/usr/share/vulkan/icd.d/lvp_icd.${ARCH}.json"
    if [ -f "$ICD_FILE" ]; then
        echo "PASS: Vulkan ICD file exists at $ICD_FILE"
    else
        echo "FAIL: Vulkan ICD file not found at $ICD_FILE"
        echo "Available ICD files:"
        ls -la /usr/share/vulkan/icd.d/ 2>/dev/null || echo "  (directory not found)"
    fi
'

# Test 4: Check environment variables
echo ""
echo "--- Test: Environment variables ---"
docker exec "$CONTAINER_NAME" bash -c '
    [ "$DISPLAY" = ":99" ] && echo "PASS: DISPLAY=:99" || echo "FAIL: DISPLAY=$DISPLAY (expected :99)"
    [ "$LIBGL_ALWAYS_SOFTWARE" = "1" ] && echo "PASS: LIBGL_ALWAYS_SOFTWARE=1" || echo "FAIL: LIBGL_ALWAYS_SOFTWARE=$LIBGL_ALWAYS_SOFTWARE"
'

# Test 5: Check xvfb is installed
echo ""
echo "--- Test: Xvfb installation ---"
docker exec "$CONTAINER_NAME" bash -c 'command -v Xvfb && echo "PASS: Xvfb installed" || echo "FAIL: Xvfb not found"'

# Test 6: Run Playwright WebGL check (the real test!)
echo ""
echo "--- Test: Playwright WebGL rendering ---"
docker exec "$CONTAINER_NAME" bash -c '
    source ~/.nvm/nvm.sh

    # Start Xvfb in background (as root it would create /tmp/.X11-unix, but we can ignore the warning)
    Xvfb :99 -screen 0 1920x1080x24 2>/dev/null &
    XVFB_PID=$!
    sleep 2

    # Create a temp project with playwright
    mkdir -p /tmp/webgl-test
    cd /tmp/webgl-test

    cat > package.json << "PKG"
{"name":"webgl-test","type":"module","dependencies":{"playwright":"*"}}
PKG

    cat > test.mjs << "SCRIPT"
import { chromium } from "playwright";

// Flags for WebGL + WebGPU support in headless mode
// See: https://developer.chrome.com/blog/supercharge-web-ai-testing
const browser = await chromium.launch({
    headless: true,
    args: [
        "--no-sandbox",
        "--disable-setuid-sandbox",
        // WebGL flags
        "--use-gl=angle",
        "--use-angle=swiftshader-webgl",
        "--enable-webgl",
        // WebGPU flags (requires Vulkan backend)
        "--enable-unsafe-webgpu",
        "--enable-features=Vulkan,UseSkiaRenderer,VulkanFromANGLE",
        "--use-vulkan=swiftshader",
        "--disable-vulkan-surface",
        // Ignore GPU blocklist for software rendering
        "--ignore-gpu-blocklist",
    ]
});

const page = await browser.newPage();

// Check WebGL support
const webglResult = await page.evaluate(() => {
    const canvas = document.createElement("canvas");
    const gl = canvas.getContext("webgl") || canvas.getContext("experimental-webgl");
    if (!gl) return { supported: false, renderer: null };
    const debugInfo = gl.getExtension("WEBGL_debug_renderer_info");
    return {
        supported: true,
        renderer: debugInfo ? gl.getParameter(debugInfo.UNMASKED_RENDERER_WEBGL) : "unknown",
        vendor: debugInfo ? gl.getParameter(debugInfo.UNMASKED_VENDOR_WEBGL) : "unknown"
    };
});

// Check WebGPU support
const webgpuResult = await page.evaluate(async () => {
    if (!navigator.gpu) return { supported: false, reason: "navigator.gpu not available" };
    try {
        const adapter = await navigator.gpu.requestAdapter();
        if (!adapter) return { supported: false, reason: "no adapter returned" };
        const info = await adapter.requestAdapterInfo();
        return {
            supported: true,
            vendor: info.vendor || "unknown",
            architecture: info.architecture || "unknown",
            device: info.device || "unknown",
            description: info.description || "unknown"
        };
    } catch (e) {
        return { supported: false, reason: e.message };
    }
});

console.log("WebGL:", JSON.stringify(webglResult));
console.log("WebGPU:", JSON.stringify(webgpuResult));

await browser.close();

// Exit with appropriate code - pass if WebGL works (WebGPU is bonus)
process.exit(webglResult.supported ? 0 : 1);
SCRIPT

    # Use npx to run with the globally installed playwright
    # Link global playwright to local node_modules
    npm link playwright 2>/dev/null || npm install playwright 2>/dev/null

    # First try with bundled Chromium
    echo "Testing with Playwright Chromium..."
    node test.mjs
    CHROMIUM_EXIT=$?

    # Try WebGPU with Chrome for Testing (only available on x86_64)
    ARCH=$(uname -m)
    echo ""
    if [ "$ARCH" = "x86_64" ]; then
        echo "Architecture: x86_64 - Chrome for Testing available"
        echo "Installing Chrome for Testing (has WebGPU support)..."
        npx playwright install chrome 2>/dev/null

        # Create Chrome-specific test
        cat > test-chrome.mjs << "CHROME_SCRIPT"
import { chromium } from "playwright";

const browser = await chromium.launch({
    channel: "chrome",  // Use Chrome for Testing instead of Chromium
    headless: true,
    args: [
        "--no-sandbox",
        "--disable-setuid-sandbox",
        "--enable-unsafe-webgpu",
        "--enable-features=Vulkan,UseSkiaRenderer",
        "--use-vulkan=swiftshader",
        "--disable-vulkan-surface",
        "--ignore-gpu-blocklist",
    ]
});

const page = await browser.newPage();

const webgpuResult = await page.evaluate(async () => {
    if (!navigator.gpu) return { supported: false, reason: "navigator.gpu not available" };
    try {
        const adapter = await navigator.gpu.requestAdapter();
        if (!adapter) return { supported: false, reason: "no adapter returned" };
        const info = await adapter.requestAdapterInfo();
        return {
            supported: true,
            vendor: info.vendor || "unknown",
            architecture: info.architecture || "unknown",
            device: info.device || "unknown",
            description: info.description || "unknown"
        };
    } catch (e) {
        return { supported: false, reason: e.message };
    }
});

console.log("WebGPU (Chrome):", JSON.stringify(webgpuResult));
await browser.close();
process.exit(webgpuResult.supported ? 0 : 1);
CHROME_SCRIPT

        echo "Testing WebGPU with Chrome for Testing..."
        node test-chrome.mjs 2>/dev/null && echo "PASS: WebGPU works with Chrome!" || echo "FAIL: WebGPU test failed"
    else
        echo "Architecture: $ARCH - Chrome for Testing NOT available"
        echo "INFO: WebGPU requires Chrome for Testing, which is only available for x86_64 Linux"
        echo "      See: https://github.com/GoogleChromeLabs/chrome-for-testing/issues/1"
        echo "SKIP: WebGPU test skipped (ARM64 limitation)"
    fi
    EXIT_CODE=$?

    # Cleanup
    kill $XVFB_PID 2>/dev/null || true

    if [ $EXIT_CODE -eq 0 ]; then
        echo "PASS: WebGL is working!"
    else
        echo "FAIL: WebGL test failed"
    fi

    exit $EXIT_CODE
'

WEBGL_EXIT=$?

echo ""
echo "[4/4] Cleanup..."
docker rm -f "$CONTAINER_NAME" 2>/dev/null || true

echo ""
echo "=== Test Summary ==="
if [ $WEBGL_EXIT -eq 0 ]; then
    echo "SUCCESS: All WebGL tests passed!"
    echo ""
    echo "You can now rebuild @vibe-box with:"
    echo "  vm snapshot create @vibe-box --from-dockerfile Dockerfile.vibe --force"
    exit 0
else
    echo "FAILURE: WebGL tests did not pass"
    echo "Review the output above for details"
    exit 1
fi
