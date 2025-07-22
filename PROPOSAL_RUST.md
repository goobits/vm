# PROPOSAL: Rewrite `vm` Tool in Rust

## Overview
This proposal outlines a plan to rewrite the entire `vm.sh` shell script and its associated utilities into a single, compiled Rust application.

## Problem Statement
The current shell-based implementation is powerful but faces several limitations that will hinder future growth:
- **Fragility:** Shell scripts can be brittle and difficult to debug, especially when handling complex logic like configuration merging and command execution.
- **Dependency Management:** The tool relies on external dependencies like `yq`, which must be installed and managed by the user, creating friction during setup.
- **Testing:** While possible, writing comprehensive, isolated unit tests for complex shell scripts is significantly harder than in a compiled language.
- **Maintainability:** As features are added, the shell codebase will become increasingly complex and difficult for new contributors to understand and modify safely.
- **Error Handling:** Rust's robust type system and explicit error handling (Result/Option) are far superior to the manual error checking required in shell.
- **Performance:** While not currently a major issue, a compiled Rust binary will be faster.

## Solution: Rewrite in Rust
We will incrementally rewrite the tool in Rust, creating a single, statically-linked command-line application (`vm`).

### Key Benefits
- **Reliability:** Rust's compiler enforces memory and thread safety, eliminating entire classes of bugs.
- **Zero Dependencies:** The final product will be a single binary. Users can just download it and run it without needing to install `yq`, Python, Node, or any other runtime or library.
- **Maintainability:** A structured, modular Rust codebase will be far easier to manage, test, and extend over the long term.
- **First-Class Tooling:** We can leverage Rust's excellent ecosystem for argument parsing (clap), serialization (serde for YAML/JSON), testing, and more.
- **Cross-Platform:** While our primary target is Linux/macOS, a Rust codebase is much easier to compile for other platforms (like Windows) in the future.

## Implementation Plan (High-Level)

### Phase 1: Configuration and Parsing
- Replicate the 3-tier YAML configuration logic in Rust using `serde` and `figment`.
- The Rust binary's first job will be to parse all `*.yml` files and output the final, merged JSON config, replacing the need for `yq` and `generate-config.sh`.
- The `vm.sh` script will call this new Rust binary during this transitional phase.

### Phase 2: Command Logic
- Port the logic for core commands (`create`, `start`, `stop`, `destroy`) from the shell script into the Rust application.
- This involves handling the Docker/Vagrant provider logic and executing external commands.

### Phase 3: Deprecation
- Once all functionality is ported, the `vm.sh` script will be deprecated and replaced entirely by the new `vm` binary.

This strategic rewrite will provide a solid foundation for the future of the Goobits VM tool, making it more robust, secure, and easier to use and contribute to.
