#!/usr/bin/env bash
set -e

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}Running duplicate code detection and complexity analysis...${NC}\n"

# Run jscpd (duplicate detection)
echo -e "${YELLOW}=== jscpd: Duplicate Code Detection ===${NC}"
jscpd -c .jscpd.json rust/
echo ""

# Run rust-code-analysis (complexity metrics)
echo -e "${YELLOW}=== rust-code-analysis: Complexity Metrics ===${NC}"
rust-code-analysis-cli --metrics -p rust/ -O json > .build/rust-code-analysis-report.json
echo "✓ Metrics saved to .build/rust-code-analysis-report.json"
echo ""

# Find high complexity functions (Cyclomatic Complexity > 10)
echo -e "${YELLOW}=== High Complexity Functions (CC > 10) ===${NC}"
jq -r '
  .. |
  objects |
  select(.metrics.cyclomatic.sum? > 10) |
  "\(.name // "unknown"):\(.start_line) - CC: \(.metrics.cyclomatic.sum) | Cognitive: \(.metrics.cognitive.sum)"
' .build/rust-code-analysis-report.json | head -20

echo ""
echo -e "${GREEN}Reports location:${NC}"
echo "  - Duplicates (HTML): .build/jscpd-report/html/index.html"
echo "  - Duplicates (JSON):  .build/jscpd-report/jscpd-report.json"
echo "  - Complexity (JSON):  .build/rust-code-analysis-report.json"
