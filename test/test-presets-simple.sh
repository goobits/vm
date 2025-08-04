#!/bin/bash
# Simple test to verify preset system functionality

# Basic test for project detection
cd /workspace
source shared/project-detector.sh

# Test with a temporary directory
mkdir -p /tmp/test-react
cat > /tmp/test-react/package.json << 'EOF'
{
  "name": "test-app",
  "dependencies": {
    "react": "^18.0.0"
  }
}
EOF

echo "Testing React detection:"
detected=$(detect_project_type /tmp/test-react)
echo "Detected: $detected"

if [ "$detected" = "react" ]; then
    echo "✓ React detection works!"
else
    echo "✗ React detection failed"
fi

# Clean up
rm -rf /tmp/test-react

echo "Basic test completed."