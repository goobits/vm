#!/bin/bash\n\n# This script is a wrapper for the Rust-based vm binary.\n\nSCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"\n"$SCRIPT_DIR/rust/target/release/vm" "$@"
