# Proposal: Major Dependency Updates

This document outlines the proposed major version updates for several dependencies in the workspace.

## 1. `thiserror`

*   **Current Version:** `2.0`
*   **Proposed Version:** `1.0.63`
*   **Reasoning:** The version `2.0` in our `Cargo.toml` appears to be a typo. The latest version of `thiserror` on crates.io is `1.0.63`. This is a straightforward correction.
*   **Breaking Changes:** None expected, as this is a correction to a non-existent version.

## 2. `indexmap`

*   **Current Version:** `2.0`
*   **Proposed Version:** `2.11.4`
*   **Reasoning:** The latest version of `indexmap` includes new features and performance improvements.
*   **Breaking Changes:** The primary breaking change is the deprecation of the `remove` method in favor of `shift_remove` and `swap_remove` in version 2.2.0. A codebase search will be required to identify and update all instances of `remove`.

## 3. `uuid`

*   **Current Version:** `1.0`
*   **Proposed Version:** `1.10.0`
*   **Reasoning:** The latest version of `uuid` includes support for RFC 9562, new features like `zerocopy`, and an updated MSRV.
*   **Breaking Changes:** The MSRV has been updated to 1.60. No other breaking API changes have been identified, but a thorough review of the changelog is recommended before proceeding with the update.
