#!/usr/bin/env bash
set -euo pipefail

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}Running duplicate code detection...${NC}\n"

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo -e "${RED}Missing required tool: $1${NC}" >&2
    echo "$2" >&2
    exit 127
  fi
}

require_tool jscpd "Install with: npm install -g jscpd"

# Run jscpd (duplicate detection)
echo -e "${YELLOW}=== jscpd: Duplicate Code Detection ===${NC}"
jscpd -c .jscpd.json rust/
