```
================================================================================
                          📍 PROJECT CODEMAP
================================================================================

PROJECT SUMMARY
---------------
  Name:         Goobits VM
  Type:         CLI Tool / Development Environment Manager
  Language:     Rust
  Framework:    Clap (CLI), Docker/libvirt (virtualization)
  Entry Point:  rust/vm/src/main.rs

  Total Files:  133 (.rs files)
  Total LOC:    248,661

================================================================================

🏗️ ARCHITECTURE OVERVIEW
------------------------

┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  VM CLI     │────▶│   Provider  │────▶│  Container/ │
│   (Rust)    │     │  (Docker/   │     │     VM      │
└─────────────┘     │   libvirt)  │     └─────────────┘
        │           └─────────────┘            │
    [Commands]           │                  [Ansible]
        │           [Templates]            [Playbooks]
        ▼                │                      │
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Detector   │     │   Config    │     │   Services  │
│ (Framework) │     │   (YAML)    │     │  (DB/Redis) │
└─────────────┘     └─────────────┘     └─────────────┘

Key Patterns:
  • Workspace: Rust monorepo with 10 crates
  • Provider: Docker (fast) or libvirt (isolated) backends
  • Config: YAML-based VM definitions & service configs
  • Detection: Auto-detects 30+ frameworks/languages

================================================================================

📁 DIRECTORY STRUCTURE
----------------------

[root]/
├── rust/                   [Rust monorepo workspace]
│   ├── vm/ [25]           [Main CLI binary]
│   │   ├── src/           [CLI commands & core logic]
│   │   └── tests/         [Integration tests]
│   ├── vm-provider/ [38]  [Docker/libvirt backends]
│   │   ├── src/docker/    [Docker provider impl]
│   │   ├── src/libvirt/   [Libvirt provider impl]
│   │   └── src/resources/ [Ansible playbooks]
│   ├── vm-detector/ [10]  [Framework detection]
│   │   └── src/tests/     [Framework test fixtures]
│   ├── vm-config/ [8]     [Config management]
│   │   └── configs/       [Default configurations]
│   ├── vm-installer/ [3]  [Installation logic]
│   ├── vm-ports/ [5]      [Port management]
│   ├── vm-pkg/ [6]        [Package management]
│   ├── vm-temp/ [4]       [Temp VM handling]
│   ├── vm-common/ [4]     [Shared utilities]
│   └── tests/             [Cross-crate integration]
├── configs/               [User config templates]
│   ├── languages/         [Language-specific configs]
│   ├── services/          [Database/service configs]
│   └── presets/           [Framework presets]
├── bin/                   [Shell scripts & helpers]
├── docs/                  [Documentation]
└── vm.yaml               [Default VM definition]

================================================================================

🔑 KEY FILES (Start Here)
-------------------------

ENTRY POINTS:
  • [rust/vm/src/main.rs]              - CLI entry, command router
  • [rust/vm/src/cli/mod.rs]           - Command definitions
  • [install.sh]                        - Installation script

CORE LOGIC:
  • [rust/vm-provider/src/lib.rs]      - Provider abstraction
  • [rust/vm-detector/src/detector.rs] - Framework detection
  • [rust/vm-config/src/config.rs]     - Config parsing/validation

CONFIGURATION:
  • [vm.yaml]                          - Default VM definition
  • [defaults.yaml]                    - Global defaults
  • [configs/services/*.yaml]          - Service templates

ANSIBLE/PROVISIONING:
  • [rust/vm-provider/src/resources/ansible/playbook.yml] - Main playbook
  • [rust/vm-provider/src/resources/services/*.yml]       - Service defs

================================================================================

🔄 DATA FLOW
------------

1. Command Entry:
   [vm/main.rs] → [cli/mod.rs] → [commands/*.rs]

2. VM Creation:
   [commands/create.rs] → [vm-detector] → [vm-provider] → [Docker/libvirt]

3. Configuration:
   [vm.yaml] → [vm-config] → [provider] → [Ansible playbook]

Key Relationships:
  • [vm] depends on → [vm-provider], [vm-config], [vm-detector]
  • [vm-provider] depends on → [vm-common], [bollard/virt]
  • [vm-detector] uses → [framework patterns], [file signatures]

================================================================================

📦 DEPENDENCIES
---------------

PRODUCTION:
  • [clap]         - CLI framework
  • [bollard]      - Docker API client
  • [virt]         - libvirt bindings
  • [serde]        - Serialization
  • [tokio]        - Async runtime
  • [tracing]      - Logging/telemetry

DEVELOPMENT:
  • [cargo test]   - Testing framework
  • [tempfile]     - Test file handling
  • [serial_test]  - Sequential test execution

External Services:
  • [Docker]       - Container provider (configured in: provider/docker)
  • [libvirt]      - VM provider (configured in: provider/libvirt)
  • [Ansible]      - Provisioning (playbooks in: resources/ansible)

================================================================================

🎯 COMMON TASKS
---------------

To understand VM creation:
  Start with: [vm/src/commands/create.rs] → [vm-provider/src/lib.rs] →
  [provider/docker/mod.rs]

To modify detection logic:
  Core files: [vm-detector/src/detector.rs], [detector/patterns.rs]
  Tests: [vm-detector/src/tests/*.rs]

To add new service:
  1. Create config in [configs/services/]
  2. Add to [resources/services/service_definitions.yml]
  3. Update playbook in [resources/ansible/]
  4. Add detection in [vm-detector/]
  5. Write tests in [tests/]

================================================================================

⚡ QUICK REFERENCE
-----------------

Naming Conventions:
  • Files:       snake_case
  • Structs:     PascalCase
  • Functions:   snake_case
  • Constants:   UPPER_SNAKE_CASE

Commands:
  • Build:       cargo build --workspace
  • Test:        cargo test --workspace
  • Install:     ./install.sh
  • Run:         vm create / vm list / vm ssh

VM Commands:
  • Create:      vm create [name]
  • List:        vm list
  • SSH:         vm ssh [name]
  • Delete:      vm delete [name]
  • Temp:        vm temp [folder]

================================================================================

⚠️ GOTCHAS & NOTES
------------------

• TEST_MUTEX required for config/workflow tests (env modification)
• Docker provider requires daemon running
• libvirt provider needs KVM/QEMU installed
• File sync uses ansible-rsync for performance
• Port registry prevents conflicts between VMs
• Temp VMs auto-delete after 24h or on exit

================================================================================
```