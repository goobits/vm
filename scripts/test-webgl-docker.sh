#!/bin/bash
# Test WebGL/WebGPU support in Docker before rebuilding @vibe-box
# Usage: ./scripts/test-webgl-docker.sh

set -e

IMAGE_NAME="vibe-box-test"
CONTAINER_NAME="vibe-webgl-test"

echo "=== WebGL/WebGPU Docker Test ==="
echo ""

# Build test image
echo "[1/4] Building test image from Dockerfile.vibe..."
docker build -t "$IMAGE_NAME" -f Dockerfile.vibe .

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
    libs="libegl libgl libgles libvulkan"
    for lib in $libs; do
        if ldconfig -p 2>/dev/null | grep -q "$lib" || find /usr/lib -name "*${lib}*" 2>/dev/null | head -1 | grep -q .; then
            echo "PASS: $lib found"
        else
            echo "FAIL: $lib not found"
        fi
    done
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

    # Start Xvfb in background
    Xvfb :99 -screen 0 1920x1080x24 &
    XVFB_PID=$!
    sleep 2

    # Create test script
    cat > /tmp/webgl-test.js << "SCRIPT"
const { chromium } = require("playwright");

(async () => {
    const browser = await chromium.launch({
        headless: true,
        args: [
            "--no-sandbox",
            "--disable-setuid-sandbox",
            "--use-gl=angle",
            "--use-angle=swiftshader-webgl",
            "--enable-webgl",
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
        if (!navigator.gpu) return { supported: false };
        try {
            const adapter = await navigator.gpu.requestAdapter();
            if (!adapter) return { supported: false, reason: "no adapter" };
            const info = await adapter.requestAdapterInfo();
            return { supported: true, vendor: info.vendor, architecture: info.architecture };
        } catch (e) {
            return { supported: false, reason: e.message };
        }
    });

    console.log("WebGL:", JSON.stringify(webglResult));
    console.log("WebGPU:", JSON.stringify(webgpuResult));

    await browser.close();

    // Exit with appropriate code
    process.exit(webglResult.supported ? 0 : 1);
})();
SCRIPT

    node /tmp/webgl-test.js
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
