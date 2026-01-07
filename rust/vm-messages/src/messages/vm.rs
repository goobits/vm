//! VM lifecycle messages (create, start, stop, destroy, restart, apply, exec, logs, list, ssh, etc.)

pub struct VmMessages {
    // ============================================================================
    // VM General (shared across vm commands, alphabetically sorted)
    // ============================================================================
    pub ambiguous: &'static str,
    pub is_running: &'static str,
    pub is_stopped: &'static str,
    pub not_found: &'static str,
    pub using: &'static str,

    // ============================================================================
    // VM Create Messages (alphabetically sorted)
    // ============================================================================
    pub create_force_recreating: &'static str,
    pub create_force_recreating_instance: &'static str,
    pub create_header: &'static str,
    pub create_header_instance: &'static str,
    pub create_info_block: &'static str,
    pub create_multiinstance_warning: &'static str,
    pub create_ports_label: &'static str,
    pub create_progress: &'static str,
    pub create_success: &'static str,
    pub create_troubleshooting: &'static str,

    // ============================================================================
    // VM Destroy Messages (alphabetically sorted)
    // ============================================================================
    pub destroy_cancelled: &'static str,
    pub destroy_cleanup_already_removed: &'static str,
    pub destroy_confirm: &'static str,
    pub destroy_confirm_prompt: &'static str,
    pub destroy_force: &'static str,
    pub destroy_info_block: &'static str,
    pub destroy_progress: &'static str,
    pub destroy_success: &'static str,

    // ============================================================================
    // VM Start Messages (alphabetically sorted)
    // ============================================================================
    pub start_already_running: &'static str,
    pub start_header: &'static str,
    pub start_info_block: &'static str,
    pub start_success: &'static str,
    pub start_troubleshooting: &'static str,

    // ============================================================================
    // VM Stop Messages (alphabetically sorted)
    // ============================================================================
    pub stop_force_header: &'static str,
    pub stop_force_success: &'static str,
    pub stop_force_troubleshooting: &'static str,
    pub stop_header: &'static str,
    pub stop_restart_hint: &'static str,
    pub stop_success: &'static str,
    pub stop_troubleshooting: &'static str,

    // ============================================================================
    // VM Restart Messages
    // ============================================================================
    pub restart_header: &'static str,
    pub restart_success: &'static str,
    pub restart_troubleshooting: &'static str,

    // ============================================================================
    // VM Apply Messages
    // ============================================================================
    pub apply_header: &'static str,
    pub apply_hint: &'static str,
    pub apply_progress: &'static str,
    pub apply_success: &'static str,
    pub apply_troubleshooting: &'static str,

    // ============================================================================
    // VM Exec Messages
    // ============================================================================
    pub exec_header: &'static str,
    pub exec_separator: &'static str,
    pub exec_success: &'static str,
    pub exec_failed: &'static str,
    pub exec_troubleshooting: &'static str,

    // ============================================================================
    // VM Logs Messages
    // ============================================================================
    pub logs_header: &'static str,
    pub logs_separator: &'static str,
    pub logs_footer: &'static str,
    pub logs_troubleshooting: &'static str,

    // ============================================================================
    // VM List Messages
    // ============================================================================
    pub list_empty: &'static str,
    pub list_empty_provider: &'static str,
    pub list_table_header: &'static str,
    pub list_table_separator: &'static str,

    // ============================================================================
    // VM Snapshot Messages
    // ============================================================================
    pub snapshot_list_empty: &'static str,
    pub snapshot_list_table_header: &'static str,
    pub snapshot_list_table_separator: &'static str,

    // ============================================================================
    // VM SSH Messages
    // ============================================================================
    pub ssh_connecting: &'static str,
    pub ssh_disconnected: &'static str,
    pub ssh_vm_not_found: &'static str,
    pub ssh_create_prompt: &'static str,
    pub ssh_creating: &'static str,
    pub ssh_create_success: &'static str,
    pub ssh_create_failed: &'static str,
    pub ssh_not_running: &'static str,
    pub ssh_connection_lost: &'static str,
    pub ssh_session_ended: &'static str,
    pub ssh_start_hint: &'static str,
    pub ssh_start_prompt: &'static str,
    pub ssh_start_aborted: &'static str,
    pub ssh_starting: &'static str,
    pub ssh_start_failed: &'static str,
    pub ssh_reconnecting: &'static str,

    // ============================================================================
    // VM Destroy Enhanced (Cross-Provider) Messages
    // ============================================================================
    pub destroy_cross_no_instances: &'static str,
    pub destroy_cross_list_header: &'static str,
    pub destroy_cross_list_item: &'static str,
    pub destroy_cross_confirm_prompt: &'static str,
    pub destroy_cross_cancelled: &'static str,
    pub destroy_cross_progress: &'static str,
    pub destroy_cross_success_item: &'static str,
    pub destroy_cross_failed: &'static str,
    pub destroy_cross_complete: &'static str,

    // ============================================================================
    // VM Doctor Messages
    // ============================================================================
    pub doctor_header: &'static str,
    pub doctor_config_section: &'static str,
    pub doctor_deps_section: &'static str,
    pub doctor_services_section: &'static str,
    pub doctor_summary_separator: &'static str,
    pub doctor_all_passed: &'static str,
    pub doctor_some_failed: &'static str,
    pub doctor_config_loaded: &'static str,
    pub doctor_config_valid: &'static str,
    pub doctor_config_invalid: &'static str,
    pub doctor_config_incomplete: &'static str,
    pub doctor_config_complete: &'static str,
    pub doctor_config_not_found: &'static str,
    pub doctor_config_not_found_hint: &'static str,
    pub doctor_config_load_failed: &'static str,
    pub doctor_docker_found: &'static str,
    pub doctor_docker_not_found: &'static str,
    pub doctor_docker_not_found_hint: &'static str,
    pub doctor_docker_check_failed: &'static str,
    pub doctor_docker_daemon_running: &'static str,
    pub doctor_docker_daemon_not_running: &'static str,
    pub doctor_docker_daemon_not_running_hint: &'static str,
    pub doctor_docker_daemon_check_failed: &'static str,
    pub doctor_git_found: &'static str,
    pub doctor_git_not_found: &'static str,
    pub doctor_git_not_found_hint: &'static str,
    pub doctor_git_check_failed: &'static str,
    pub doctor_auth_healthy: &'static str,
    pub doctor_auth_not_responding: &'static str,
    pub doctor_auth_not_responding_hint: &'static str,
    pub doctor_auth_check_failed: &'static str,
    pub doctor_pkg_healthy: &'static str,
    pub doctor_pkg_not_responding: &'static str,
    pub doctor_pkg_not_responding_hint: &'static str,
    pub doctor_pkg_check_failed: &'static str,
    pub doctor_registry_healthy: &'static str,
    pub doctor_registry_not_responding_active: &'static str,
    pub doctor_registry_not_responding_hint: &'static str,
    pub doctor_registry_not_running_info: &'static str,
    pub doctor_registry_check_failed_active: &'static str,
    pub doctor_registry_check_skipped: &'static str,

    // ============================================================================
    // VM Auth Proxy Messages
    // ============================================================================
    pub auth_status_header: &'static str,
    pub auth_reference_count: &'static str,
    pub auth_registered_vms: &'static str,
    pub auth_not_managed: &'static str,
    pub auth_server_url: &'static str,
    pub auth_health_ok: &'static str,
    pub auth_health_failed: &'static str,
    pub auth_auto_managed_info: &'static str,
    pub auth_adding_secret: &'static str,
    pub auth_secret_added: &'static str,
    pub auth_removing_secret: &'static str,
    pub auth_secret_removed: &'static str,
    pub auth_interactive_header: &'static str,
    pub auth_interactive_success: &'static str,

    // ============================================================================
    // VM Update Messages
    // ============================================================================
    pub update_current_version: &'static str,
    pub update_target_version: &'static str,
    pub update_via_cargo: &'static str,
    pub update_cargo_success: &'static str,
    pub update_cargo_failed: &'static str,
    pub update_downloading_github: &'static str,
    pub update_fetching_release: &'static str,
    pub update_release_fetch_failed: &'static str,
    pub update_check_version_hint: &'static str,
    pub update_platform_not_found: &'static str,
    pub update_downloading_binary: &'static str,
    pub update_download_failed: &'static str,
    pub update_extracting: &'static str,
    pub update_extract_failed: &'static str,
    pub update_binary_not_found: &'static str,
    pub update_backing_up: &'static str,
    pub update_installing: &'static str,
    pub update_success: &'static str,
    pub update_new_version: &'static str,

    // ============================================================================
    // VM Dry Run Messages
    // ============================================================================
    pub dry_run_header: &'static str,
    pub dry_run_command: &'static str,
    pub dry_run_config: &'static str,
    pub dry_run_complete: &'static str,

    // VM Package Registry Messages
    pub pkg_registry_status_header: &'static str,
    pub pkg_registry_reference_count: &'static str,
    pub pkg_registry_registered_vms: &'static str,
    pub pkg_registry_not_managed: &'static str,
    pub pkg_registry_health_ok: &'static str,
    pub pkg_registry_health_failed: &'static str,
    pub pkg_registry_auto_managed_info: &'static str,
    pub pkg_publishing: &'static str,
    pub pkg_removing: &'static str,
    pub pkg_config_header: &'static str,
    pub pkg_config_port: &'static str,
    pub pkg_config_host: &'static str,
    pub pkg_config_fallback: &'static str,
    pub pkg_config_changes_hint: &'static str,
    pub pkg_config_setting: &'static str,
    pub pkg_use_bash_config: &'static str,
    pub pkg_use_fish_config: &'static str,
    pub pkg_use_unsupported: &'static str,
    pub pkg_version_mismatch: &'static str,
    pub pkg_restarting: &'static str,
    pub pkg_server_starting: &'static str,
    pub pkg_server_logs: &'static str,
    pub pkg_server_started_info: &'static str,
    pub pkg_serve_starting: &'static str,

    // VM Uninstall Messages
    pub uninstall_header: &'static str,
    pub uninstall_will_remove: &'static str,
    pub uninstall_binary: &'static str,
    pub uninstall_config_files: &'static str,
    pub uninstall_config_file_item: &'static str,
    pub uninstall_path_entries: &'static str,
    pub uninstall_path_entry_item: &'static str,
    pub uninstall_cancelled: &'static str,
    pub uninstall_progress: &'static str,
    pub uninstall_removing_file: &'static str,
    pub uninstall_cleaned_path: &'static str,
    pub uninstall_complete_instructions: &'static str,
    pub uninstall_remove_cargo: &'static str,
    pub uninstall_remove_sudo: &'static str,
    pub uninstall_remove_no_sudo_hint: &'static str,
    pub uninstall_remove_no_sudo: &'static str,
    pub uninstall_remove_generic: &'static str,
    pub uninstall_thank_you: &'static str,
}

pub const VM_MESSAGES: VmMessages = VmMessages {
    // VM General
    ambiguous: "\n⚠️  Multiple VMs found with similar names:",
    is_running: "✅ VM '{name}' is running",
    is_stopped: "🔴 VM '{name}' is stopped",
    not_found: "🔍 No running VM found with that name.",
    using: "📍 Using: {name}",

    // VM Create
    create_force_recreating: "🔄 Force recreating '{name}'...",
    create_force_recreating_instance: "🔄 Force recreating instance '{name}'...",
    create_header: "🚀 Creating '{name}'...\n",
    create_header_instance: "🚀 Creating instance '{instance}' for project '{name}'...",
    create_info_block: "  Status:     {status}\n  Container:  {container}",
    create_multiinstance_warning: "ℹ️  Instance name '{instance}' specified but provider '{provider}' doesn't support multi-instance. Using default behavior.",
    create_ports_label: "  Ports:      {start}-{end}",
    create_progress: "  ✓ Building Docker image\n  ✓ Setting up volumes\n  ✓ Configuring network\n  ✓ Starting container\n  ✓ Running initial provisioning",
    create_success: "\n✅ Created successfully\n",
    create_troubleshooting: "\n❌ Failed to create '{name}'\n   Error: {error}\n\n💡 Try:\n   • Check Docker status: docker ps\n   • View Docker logs: docker logs\n   • Retry with force: vm create --force",

    // VM Destroy
    destroy_cancelled: "\n❌ Destruction cancelled",
    destroy_cleanup_already_removed: "✅ Container already removed, cleaning up remaining resources...\n\n  ✓ Cleaning images\n\n🔧 Cleaning up services...",
    destroy_confirm: "🗑️ Destroy VM '{name}'?\n",
    destroy_confirm_prompt: "Confirm destruction? (y/N): ",
    destroy_force: "🗑️ Destroying '{name}' (forced)\n",
    destroy_info_block: "  Status:     {status}\n  Container:  {container}\n\n⚠️  This will permanently delete:\n  • Container and all data\n  • Docker image and build cache\n",
    destroy_progress: "\n  ✓ Stopping container\n  ✓ Removing container\n  ✓ Cleaning images",
    destroy_success: "\n✅ VM destroyed",

    // VM Start
    start_already_running: "✅ VM '{name}' is already running\n\n💡 Connect with: vm ssh",
    start_header: "🚀 Starting '{name}'...",
    start_info_block: "  Status:     {status}\n  Container:  {container}",
    start_success: "✅ Started successfully\n",
    start_troubleshooting: "❌ Failed to start '{name}'\n   Error: {error}\n\n💡 Try:\n   • Check Docker status: docker ps\n   • View logs: docker logs {container}\n   • Recreate VM: vm create --force",

    // VM Stop
    stop_force_header: "⚠️  Force stopping container '{name}'...",
    stop_force_success: "✅ Container stopped\n\n🔧 Cleaning up services...",
    stop_force_troubleshooting: "❌ Failed to stop container\n   Error: {error}",
    stop_header: "🛑 Stopping '{name}'...",
    stop_restart_hint: "\n💡 Restart with: vm start",
    stop_success: "✅ Stopped successfully\n\n🔧 Cleaning up services...",
    stop_troubleshooting: "❌ Failed to stop '{name}'\n   Error: {error}",

    // VM Restart
    restart_header: "🔄 Restarting '{name}'...",
    restart_success: "✅ Restarted successfully",
    restart_troubleshooting: "\n❌ Failed to restart '{name}'\n   Error: {error}",

    // VM Apply
    apply_header: "🔧 Applying configuration to '{name}'\n",
    apply_hint: "\n💡 Changes applied to running container",
    apply_progress: "  ✓ Updating packages\n  ✓ Installing dependencies\n  ✓ Configuring services\n  ✓ Restarting services",
    apply_success: "\n✅ Configuration application complete",
    apply_troubleshooting: "\n❌ Configuration application failed\n   Error: {error}\n\n💡 Check logs: vm logs",

    // VM Exec
    exec_header: "🏃 Running in '{name}': {command}\n──────────────────────────────────────────",
    exec_separator: "──────────────────────────────────────────",
    exec_success: "✅ Command completed successfully (exit code 0)\n\n💡 Run another: vm exec <command>",
    exec_failed: "❌ Command failed\n   Error: {error}",
    exec_troubleshooting: "❌ Command failed\n   Error: {error}\n\n💡 Debug with: vm ssh",

    // VM Logs
    logs_header: "📜 Logs for '{name}' (last 50 lines)\n──────────────────────────────────────────",
    logs_separator: "──────────────────────────────────────────",
    logs_footer: "──────────────────────────────────────────\n💡 Follow live: docker logs -f {container}\n💡 Full logs: docker logs {container}",
    logs_troubleshooting: "❌ Failed to retrieve logs\n   Error: {error}",

    // VM List
    list_empty: "No VMs found",
    list_empty_provider: "No VMs found for provider '{provider}'",
    list_table_header: "INSTANCE             PROVIDER   STATUS       ID                   UPTIME     PROJECT        ",
    list_table_separator: "─────────────────────────────────────────────────────────────────────────────────────────────────────",

    // VM Snapshot
    snapshot_list_empty: "No snapshots found.",
    snapshot_list_table_header: "TYPE      NAME                 CREATED               SIZE       DESCRIPTION         ",
    snapshot_list_table_separator: "────────────────────────────────────────────────────────────────────────────────────",

    // VM SSH
    ssh_connecting: "🔗 Connecting to '{name}'...",
    ssh_disconnected: "\n👋 Disconnected from '{name}'\n💡 Reconnect with: vm ssh",
    ssh_vm_not_found: "\n🔍 VM '{name}' doesn't exist",
    ssh_create_prompt: "\nWould you like to create it now? (y/N): ",
    ssh_creating: "\n🚀 Creating '{name}'...\n\n  ✓ Building Docker image\n  ✓ Setting up volumes\n  ✓ Configuring network\n  ✓ Starting container\n  ✓ Running initial provisioning",
    ssh_create_success: "\n✅ Created successfully\n\n🔗 Connecting to '{name}'...",
    ssh_create_failed: "\n❌ Failed to create '{name}'\n   Error: {error}\n\n💡 Try:\n   • Check Docker: docker ps\n   • View logs: docker logs\n   • Manual create: vm create",
    ssh_not_running: "\n⚠️  VM '{name}' is not running",
    ssh_connection_lost: "\n⚠️  Lost connection to VM\n💡 Check if VM is running: vm status",
    ssh_session_ended: "\n⚠️  Session ended unexpectedly\n💡 Check VM status: vm status",
    ssh_start_hint: "\n💡 Start the VM with: vm start\n💡 Then reconnect with: vm ssh",
    ssh_start_prompt: "\nWould you like to start it now? (Y/n): ",
    ssh_start_aborted: "\n❌ SSH connection aborted\n💡 Start the VM manually with: vm start",
    ssh_starting: "\n🚀 Starting '{name}'...",
    ssh_start_failed: "\n❌ Failed to start '{name}': {error}\n\n💡 Try:\n   • Check Docker status: docker ps\n   • View logs: docker logs {name}-dev\n   • Recreate VM: vm create --force",
    ssh_reconnecting: "✅ Started successfully\n\n🔗 Reconnecting to '{name}'...",

    // VM Destroy Enhanced (Cross-Provider)
    destroy_cross_no_instances: "No instances found to destroy",
    destroy_cross_list_header: "Instances to destroy:",
    destroy_cross_list_item: "  {name} ({provider})",
    destroy_cross_confirm_prompt: "\nAre you sure you want to destroy {count} instance(s)? (y/N): ",
    destroy_cross_cancelled: "Destroy operation cancelled",
    destroy_cross_progress: "Destroying {name} ({provider})...",
    destroy_cross_success_item: "  ✅ Successfully destroyed {name}",
    destroy_cross_failed: "  ❌ Failed to destroy {name}: {error}",
    destroy_cross_complete: "\nDestroy operation completed:\n  Success: {success}\n  Errors: {errors}",

    // VM Doctor
    doctor_header: "🩺 VM Environment Health Check\n==============================",
    doctor_config_section: "📋 Configuration Validation:",
    doctor_deps_section: "🔧 System Dependencies:",
    doctor_services_section: "🔄 Background Services:",
    doctor_summary_separator: "==============================",
    doctor_all_passed: "✅ All checks passed! Your VM environment is healthy.",
    doctor_some_failed: "⚠️  Some checks failed. Please review the issues above.",
    doctor_config_loaded: "✅ Configuration loaded successfully",
    doctor_config_valid: "✅ Configuration validation passed",
    doctor_config_invalid: "❌ Configuration validation failed:",
    doctor_config_incomplete: "⚠️  Configuration is incomplete (missing provider or project name)",
    doctor_config_complete: "✅ Configuration is complete",
    doctor_config_not_found: "❌ No vm.yaml configuration file found",
    doctor_config_not_found_hint: "   💡 Run 'vm init' to create a configuration file",
    doctor_config_load_failed: "❌ Failed to load configuration: {error}",
    doctor_docker_found: "✅ Docker command found",
    doctor_docker_not_found: "❌ Docker command not found in PATH",
    doctor_docker_not_found_hint: "   💡 Install Docker: https://docs.docker.com/get-docker/",
    doctor_docker_check_failed: "❌ Failed to check Docker: {error}",
    doctor_docker_daemon_running: "✅ Docker daemon is running",
    doctor_docker_daemon_not_running: "❌ Docker daemon is not running",
    doctor_docker_daemon_not_running_hint: "   💡 Start Docker daemon or Docker Desktop",
    doctor_docker_daemon_check_failed: "❌ Failed to check Docker daemon: {error}",
    doctor_git_found: "✅ Git command found",
    doctor_git_not_found: "❌ Git command not found in PATH",
    doctor_git_not_found_hint: "   💡 Install Git for version control support",
    doctor_git_check_failed: "❌ Failed to check Git: {error}",
    doctor_auth_healthy: "✅ Auth proxy service is healthy",
    doctor_auth_not_responding: "❌ Auth proxy service is not responding",
    doctor_auth_not_responding_hint: "   💡 Start with: vm auth start",
    doctor_auth_check_failed: "❌ Failed to check auth proxy: {error}",
    doctor_pkg_healthy: "✅ Package server service is healthy",
    doctor_pkg_not_responding: "❌ Package server service is not responding",
    doctor_pkg_not_responding_hint: "   💡 Start with: vm pkg start",
    doctor_pkg_check_failed: "❌ Failed to check package server: {error}",
    doctor_registry_healthy: "✅ Docker registry service is healthy",
    doctor_registry_not_responding_active: "❌ Docker registry service is not responding (needed for active VMs)",
    doctor_registry_not_responding_hint: "   💡 Registry helps cache Docker images for faster VM operations",
    doctor_registry_not_running_info: "   ℹ️  Docker registry service not running (not needed without active VMs)",
    doctor_registry_check_failed_active: "❌ Failed to check Docker registry: {error}",
    doctor_registry_check_skipped: "   ℹ️  Docker registry check skipped (not needed without active VMs)",

    // VM Auth Proxy
    auth_status_header: "📊 Auth Proxy Status",
    auth_reference_count: "   Reference Count: {count} VMs",
    auth_registered_vms: "   Registered VMs:  {vms}",
    auth_not_managed: "   Status: 🔴 Not managed by service manager",
    auth_server_url: "   Server URL: {url}",
    auth_health_ok: "   Health Check: ✅ Server responding",
    auth_health_failed: "   Health Check: ❌ Server not responding",
    auth_auto_managed_info: "\n💡 Service is automatically managed by VM lifecycle\n   • Auto-starts when VM with auth_proxy: true is created\n   • Auto-stops when last VM using it is destroyed",
    auth_adding_secret: "🔐 Adding secret '{name}'...",
    auth_secret_added: "Secret added successfully",
    auth_removing_secret: "🗑️  Removing secret '{name}'...",
    auth_secret_removed: "Secret removed successfully",
    auth_interactive_header: "🔐 Interactive Secret Management\nThis will guide you through adding a new secret securely.",
    auth_interactive_success: "Secret '{name}' added successfully",

    // VM Update
    update_current_version: "Current version: v{version}",
    update_target_version: "Target version: {version}",
    update_via_cargo: "Updating via cargo...",
    update_cargo_success: "Successfully updated vm via cargo",
    update_cargo_failed: "Failed to update: {error}",
    update_downloading_github: "Downloading latest binary from GitHub releases...",
    update_fetching_release: "Fetching release information...",
    update_release_fetch_failed: "Failed to fetch release information",
    update_check_version_hint: "Check if version '{version}' exists at {repo_url}/releases",
    update_platform_not_found: "Could not find download URL for platform: {platform}",
    update_downloading_binary: "Downloading vm binary...",
    update_download_failed: "Failed to download binary",
    update_extracting: "Extracting binary...",
    update_extract_failed: "Failed to extract archive",
    update_binary_not_found: "Binary not found in archive",
    update_backing_up: "Backing up current binary...",
    update_installing: "Installing new binary...",
    update_success: "Successfully updated vm to {version}",
    update_new_version: "New version: {version}",

    // VM Dry Run
    dry_run_header: "🔍 DRY RUN MODE - showing what would be executed:",
    dry_run_command: "   Command: {command}",
    dry_run_config: "   Config: {config}",
    dry_run_complete: "🚫 Dry run complete - no commands were executed",

    // VM Package Registry
    pkg_registry_status_header: "📊 Package Registry Status",
    pkg_registry_reference_count: "   Reference Count: {count} VMs",
    pkg_registry_registered_vms: "   Registered VMs:  {vms}",
    pkg_registry_not_managed: "   Status: 🔴 Not managed by service manager",
    pkg_registry_health_ok: "   Health Check: ✅ Server responding",
    pkg_registry_health_failed: "   Health Check: ❌ Server not responding",
    pkg_registry_auto_managed_info: "\n💡 Service is automatically managed by VM lifecycle\n   • Auto-starts when VM with package_registry: true is created\n   • Auto-stops when last VM using it is destroyed",
    pkg_publishing: "📦 Publishing package to local registry...",
    pkg_removing: "🗑️  Removing package from registry...",
    pkg_config_header: "Package Registry Configuration:",
    pkg_config_port: "  Port: {port}",
    pkg_config_host: "  Host: {host}",
    pkg_config_fallback: "  Fallback: {fallback}",
    pkg_config_changes_hint: "💡 Configuration changes will take effect on next server start",
    pkg_config_setting: "⚙️  Setting {key} = {value}",
    pkg_use_bash_config: "# Package registry configuration for {shell}\nexport NPM_CONFIG_REGISTRY=http://localhost:{port}/npm/\nexport PIP_INDEX_URL=http://localhost:{port}/pypi/simple/\nexport PIP_TRUSTED_HOST=localhost\n\n# To apply: eval \"$(vm pkg use)\"",
    pkg_use_fish_config: "# Package registry configuration for fish\nset -x NPM_CONFIG_REGISTRY http://localhost:{port}/npm/\nset -x PIP_INDEX_URL http://localhost:{port}/pypi/simple/\nset -x PIP_TRUSTED_HOST localhost",
    pkg_use_unsupported: "❌ Unsupported shell: {shell}\n💡 Supported shells: bash, zsh, fish",
    pkg_version_mismatch: "⚠️  Package server version mismatch: server={server_version}, cli={cli_version}",
    pkg_restarting: "🔄 Restarting package server with new version...",
    pkg_server_starting: "🚀 Starting package registry server...",
    pkg_server_logs: "📝 Server logs: {log_path}",
    pkg_server_started_info: "💡 Server is running as a detached background process\n   Access at: http://localhost:{port}",
    pkg_serve_starting: "🚀 Starting package registry server...\n   Host: {host}\n   Port: {port}\n   Data: {data}",

    // VM Uninstall
    uninstall_header: "VM Uninstall\n============",
    uninstall_will_remove: "\nThis will remove:",
    uninstall_binary: "  • VM binary: {path}",
    uninstall_config_files: "  • Configuration files:",
    uninstall_config_file_item: "    - {path}",
    uninstall_path_entries: "  • PATH entries in:",
    uninstall_path_entry_item: "    - {path}",
    uninstall_cancelled: "Uninstall cancelled.",
    uninstall_progress: "\nUninstalling...",
    uninstall_removing_file: "  Removing {path}",
    uninstall_cleaned_path: "  Cleaned PATH from {path}",
    uninstall_complete_instructions: "\nTo complete the uninstall, run:\n",
    uninstall_remove_cargo: "  cargo uninstall vm",
    uninstall_remove_sudo: "  sudo rm {path}",
    uninstall_remove_no_sudo_hint: "\nOr without sudo if you have write permissions:",
    uninstall_remove_no_sudo: "  rm {path}",
    uninstall_remove_generic: "  rm {path}",
    uninstall_thank_you: "\nThank you for using VM!",
};
