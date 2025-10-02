//! Central registry for all user-facing message templates.
//!
//! Naming Convention:
//! - `common_*` - Shared/reusable messages across commands
//! - `{command}_{component}` - Command-specific messages (e.g., vm_create_*, vm_destroy_*)
//!
//! Multi-line messages use `\n` for better readability and fewer vm_println! calls.
//!
//! Templates use `{variable}` syntax for runtime values, which are
//! substituted by the `MessageBuilder`.

pub struct Messages {
    // ============================================================================
    // Common Messages (alphabetically sorted, shared across commands)
    // ============================================================================
    pub common_cleanup_complete: &'static str,
    pub common_configuring_services: &'static str,
    pub common_connect_hint: &'static str,
    pub common_ports_label: &'static str,
    pub common_resources_label: &'static str,
    pub common_services_cleaned: &'static str,
    pub common_services_cleanup_failed: &'static str,
    pub common_services_label: &'static str,
    pub common_status_running: &'static str,
    pub common_status_stopped: &'static str,

    // ============================================================================
    // Error Messages (alphabetically sorted)
    // ============================================================================
    pub error_command_failed: &'static str,
    pub error_debug_info: &'static str,
    pub error_generic: &'static str,
    pub error_unexpected: &'static str,
    pub error_with_context: &'static str,

    // ============================================================================
    // Generic Messages (keeping for backwards compatibility)
    // ============================================================================
    pub failed: &'static str,
    pub press_ctrl_c_to_stop: &'static str,
    pub success: &'static str,
    pub warning_generic: &'static str,

    // ============================================================================
    // VM General (shared across vm commands, alphabetically sorted)
    // ============================================================================
    pub vm_ambiguous: &'static str,
    pub vm_is_running: &'static str,
    pub vm_is_stopped: &'static str,
    pub vm_not_found: &'static str,
    pub vm_using: &'static str,

    // ============================================================================
    // VM Create Messages (alphabetically sorted)
    // ============================================================================
    pub vm_create_force_recreating: &'static str,
    pub vm_create_force_recreating_instance: &'static str,
    pub vm_create_header: &'static str,
    pub vm_create_header_instance: &'static str,
    pub vm_create_info_block: &'static str,
    pub vm_create_multiinstance_warning: &'static str,
    pub vm_create_ports_label: &'static str,
    pub vm_create_progress: &'static str,
    pub vm_create_success: &'static str,
    pub vm_create_troubleshooting: &'static str,

    // ============================================================================
    // VM Destroy Messages (alphabetically sorted)
    // ============================================================================
    pub vm_destroy_cancelled: &'static str,
    pub vm_destroy_cleanup_already_removed: &'static str,
    pub vm_destroy_confirm: &'static str,
    pub vm_destroy_confirm_prompt: &'static str,
    pub vm_destroy_force: &'static str,
    pub vm_destroy_info_block: &'static str,
    pub vm_destroy_progress: &'static str,
    pub vm_destroy_success: &'static str,

    // ============================================================================
    // VM Start Messages (alphabetically sorted)
    // ============================================================================
    pub vm_start_already_running: &'static str,
    pub vm_start_header: &'static str,
    pub vm_start_info_block: &'static str,
    pub vm_start_success: &'static str,
    pub vm_start_troubleshooting: &'static str,

    // Config
    pub config_set_success: &'static str,
    pub config_apply_changes_hint: &'static str,
    pub config_available_presets: &'static str,
    pub config_no_changes: &'static str,
    pub config_current_configuration: &'static str,
    pub config_modify_hint: &'static str,
    pub config_unset_success: &'static str,
    pub config_preset_applied: &'static str,
    pub config_restart_hint: &'static str,
    pub config_applied_presets: &'static str,
    pub config_apply_preset_hint: &'static str,

    // Init
    pub init_welcome: &'static str,
    pub init_already_exists: &'static str,
    pub init_options_hint: &'static str,
    pub init_success: &'static str,
    pub init_next_steps: &'static str,

    // Temp VM
    pub temp_vm_status: &'static str,
    pub temp_vm_creating: &'static str,
    pub temp_vm_starting: &'static str,
    pub temp_vm_stopping: &'static str,
    pub temp_vm_destroying: &'static str,
    pub temp_vm_destroyed: &'static str,
    pub temp_vm_failed_to_start: &'static str,
    pub temp_vm_connect_hint: &'static str,
    pub temp_vm_created_with_mounts: &'static str,
    pub temp_vm_connecting: &'static str,
    pub temp_vm_auto_destroying: &'static str,
    pub temp_vm_usage_hint: &'static str,
    pub temp_vm_no_vm_found: &'static str,
    pub temp_vm_create_hint: &'static str,
    pub temp_vm_container_info: &'static str,
    pub temp_vm_provider_info: &'static str,
    pub temp_vm_project_info: &'static str,
    pub temp_vm_mounts_info: &'static str,
    pub temp_vm_auto_destroy_enabled: &'static str,
    pub temp_vm_stopped_success: &'static str,
    pub temp_vm_restart_hint: &'static str,
    pub temp_vm_failed_to_stop: &'static str,
    pub temp_vm_started_success: &'static str,
    pub temp_vm_mounts_configured: &'static str,
    pub temp_vm_restarting: &'static str,
    pub temp_vm_stopping_step: &'static str,
    pub temp_vm_starting_step: &'static str,
    pub temp_vm_services_ready: &'static str,
    pub temp_vm_restarted_success: &'static str,
    pub temp_vm_mounts_active: &'static str,
    pub temp_vm_failed_to_restart: &'static str,
    pub temp_vm_mount_added: &'static str,
    pub temp_vm_updating_container: &'static str,
    pub temp_vm_mount_applied: &'static str,
    pub temp_vm_mount_source: &'static str,
    pub temp_vm_mount_target: &'static str,
    pub temp_vm_mount_access: &'static str,
    pub temp_vm_view_mounts_hint: &'static str,
    pub temp_vm_mounts_removed: &'static str,
    pub temp_vm_all_mounts_removed: &'static str,
    pub temp_vm_add_mounts_hint: &'static str,
    pub temp_vm_mount_removed: &'static str,
    pub temp_vm_view_remaining_hint: &'static str,
    pub temp_vm_unmount_required: &'static str,
    pub temp_vm_unmount_options: &'static str,
    pub temp_vm_unmount_specific: &'static str,
    pub temp_vm_unmount_all: &'static str,
    pub temp_vm_no_mounts: &'static str,
    pub temp_vm_add_mount_hint: &'static str,
    pub temp_vm_current_mounts: &'static str,
    pub temp_vm_mount_summary: &'static str,
    pub temp_vm_list_header: &'static str,
    pub temp_vm_list_item: &'static str,
    pub temp_vm_list_project: &'static str,
    pub temp_vm_list_mounts: &'static str,
    pub temp_vm_mount_removed_detail: &'static str,
    pub temp_vm_mount_display_item: &'static str,
    pub temp_vm_list_created_date: &'static str,
    pub temp_vm_list_empty: &'static str,
    pub temp_vm_list_create_hint: &'static str,
    pub temp_vm_confirm_add_mount: &'static str,
    pub temp_vm_confirm_remove_all_mounts: &'static str,
    pub temp_vm_confirm_remove_mount: &'static str,

    // Docker
    pub docker_is_running: &'static str,
    pub docker_not_running: &'static str,
    pub docker_build_failed: &'static str,
    pub docker_build_success: &'static str,

    // Installer & Dependencies
    pub installer_checking_dependencies: &'static str,
    pub installer_installing: &'static str,
    pub installer_complete: &'static str,
    pub installer_help_hint: &'static str,
    pub installer_path_already_configured: &'static str,
    pub installer_path_not_configured: &'static str,
    pub installer_add_to_path_hint: &'static str,
    pub installer_manual_path_hint: &'static str,

    // Package Management
    pub pkg_linking: &'static str,
    pub pkg_linked_package: &'static str,
    pub pkg_installing_local_cargo: &'static str,
    pub pkg_linking_npm: &'static str,
    pub pkg_pipx_detected: &'static str,
    pub pkg_python_editable: &'static str,
    pub pkg_installing_editable: &'static str,
    pub pkg_pipx_not_available: &'static str,
    pub pkg_no_bin_directory: &'static str,
    pub pkg_creating_wrappers: &'static str,
    pub pkg_wrapper_created: &'static str,
    pub pkg_restart_shell: &'static str,
    pub pkg_no_linked_packages: &'static str,
    pub pkg_linked_packages_header: &'static str,

    // Provider Operations
    pub provider_tart_vm_exists: &'static str,
    pub provider_tart_recreate_hint: &'static str,
    pub provider_tart_created_success: &'static str,
    pub provider_tart_connect_hint: &'static str,
    pub provider_tart_vm_created: &'static str,
    pub provider_tart_vm_recreate_hint: &'static str,
    pub provider_tart_vm_connect_hint: &'static str,
    pub provider_logs_unavailable: &'static str,
    pub provider_logs_expected_location: &'static str,
    pub provider_logs_showing: &'static str,
    pub provider_vm_not_found: &'static str,
    pub provider_provisioning_unsupported: &'static str,
    pub provider_provisioning_explanation: &'static str,

    // Audio
    pub audio_installing_pulseaudio: &'static str,
    pub audio_stopping_services: &'static str,
    pub audio_starting_services: &'static str,

    // Ports
    pub ports_no_ranges: &'static str,
    pub ports_registered_ranges: &'static str,
    pub ports_range_entry: &'static str,

    // Progress Reporter
    pub progress_phase_header: &'static str,
    pub progress_subtask: &'static str,
    pub progress_complete: &'static str,
    pub progress_warning: &'static str,
    pub progress_error: &'static str,
    pub progress_error_detail: &'static str,
    pub progress_error_hint: &'static str,

    // Status Formatter
    pub status_report_header: &'static str,
    pub status_report_separator: &'static str,
    pub status_report_name: &'static str,
    pub status_report_status: &'static str,
    pub status_report_provider: &'static str,
    pub status_report_memory: &'static str,
    pub status_report_cpus: &'static str,
}

pub const MESSAGES: Messages = Messages {
    // ============================================================================
    // Common Messages
    // ============================================================================
    common_cleanup_complete: "\n✅ Cleanup complete",
    common_configuring_services: "\n🔧 Configuring services...",
    common_connect_hint: "\n💡 Connect with: vm ssh",
    common_ports_label: "  Ports:      {start}-{end}",
    common_resources_label: "  Resources:  {cpus} CPUs, {memory}",
    common_services_cleaned: "  ✓ Services cleaned up successfully",
    common_services_cleanup_failed: "  ⚠️  Service cleanup failed: {error}",
    common_services_label: "  Services:   {services}",
    common_status_running: "🟢 Running",
    common_status_stopped: "🔴 Stopped",

    // ============================================================================
    // Error Messages
    // ============================================================================
    error_command_failed: "❌ Command failed: {command}",
    error_debug_info: "Debug info: {details}",
    error_generic: "Error: {error}",
    error_unexpected: "❌ Unexpected error occurred",
    error_with_context: "{error}",

    // ============================================================================
    // Generic Messages (keeping for backwards compatibility)
    // ============================================================================
    failed: "❌ Failed",
    press_ctrl_c_to_stop: "Press Ctrl+C to stop...",
    success: "✅ Success",
    warning_generic: "Warning: {warning}",

    // ============================================================================
    // VM General
    // ============================================================================
    vm_ambiguous: "\nMultiple VMs found with similar names:",
    vm_is_running: "✅ VM '{name}' is running",
    vm_is_stopped: "❌ VM '{name}' is stopped",
    vm_not_found: "No running VM found with that name.",
    vm_using: "Using: {name}",

    // ============================================================================
    // VM Create Messages
    // ============================================================================
    vm_create_force_recreating: "🔄 Force recreating '{name}'...",
    vm_create_force_recreating_instance: "🔄 Force recreating instance '{name}'...",
    vm_create_header: "🚀 Creating '{name}'...\n",
    vm_create_header_instance: "🚀 Creating instance '{instance}' for project '{name}'...",
    vm_create_info_block: "  Status:     {status}\n  Container:  {container}",
    vm_create_multiinstance_warning: "ℹ️  Instance name '{instance}' specified but provider '{provider}' doesn't support multi-instance. Using default behavior.",
    vm_create_ports_label: "  Ports:      {start}-{end}",
    vm_create_progress: "  ✓ Building Docker image\n  ✓ Setting up volumes\n  ✓ Configuring network\n  ✓ Starting container\n  ✓ Running initial provisioning",
    vm_create_success: "\n✅ Created successfully\n",
    vm_create_troubleshooting: "\n❌ Failed to create '{name}'\n   Error: {error}\n\n💡 Try:\n   • Check Docker status: docker ps\n   • View Docker logs: docker logs\n   • Retry with force: vm create --force",

    // ============================================================================
    // VM Destroy Messages
    // ============================================================================
    vm_destroy_cancelled: "\n❌ Destruction cancelled",
    vm_destroy_cleanup_already_removed: "✅ Container already removed, cleaning up remaining resources...\n\n  ✓ Cleaning images\n\n🔧 Cleaning up services...",
    vm_destroy_confirm: "🗑️ Destroy VM '{name}'?\n",
    vm_destroy_confirm_prompt: "Confirm destruction? (y/N): ",
    vm_destroy_force: "🗑️ Destroying '{name}' (forced)\n",
    vm_destroy_info_block: "  Status:     {status}\n  Container:  {container}\n\n⚠️  This will permanently delete:\n  • Container and all data\n  • Docker image and build cache\n",
    vm_destroy_progress: "\n  ✓ Stopping container\n  ✓ Removing container\n  ✓ Cleaning images",
    vm_destroy_success: "\n✅ VM destroyed",

    // ============================================================================
    // VM Start Messages
    // ============================================================================
    vm_start_already_running: "✅ VM '{name}' is already running\n\n💡 Connect with: vm ssh",
    vm_start_header: "🚀 Starting '{name}'...",
    vm_start_info_block: "  Status:     {status}\n  Container:  {container}",
    vm_start_success: "✅ Started successfully\n",
    vm_start_troubleshooting: "❌ Failed to start '{name}'\n   Error: {error}\n\n💡 Try:\n   • Check Docker status: docker ps\n   • View logs: docker logs {container}\n   • Recreate VM: vm create --force",

    // Config
    config_set_success: "✅ Set {field} = {value} in {path}",
    config_apply_changes_hint: "💡 Apply changes: vm restart",
    config_available_presets: "📦 Available presets:",
    config_no_changes: "   (no changes were made to the file)",
    config_current_configuration: "📋 Current configuration\n",
    config_modify_hint: "💡 Modify with: vm config set <field> <value>",
    config_unset_success: "✅ Unset {field} in {path}",
    config_preset_applied: "✅ Applied preset '{preset}' to {path}",
    config_restart_hint: "\n💡 Restart VM to apply changes: vm restart",
    config_applied_presets: "\n  Applied presets:",
    config_apply_preset_hint: "💡 Apply this preset: vm config preset {name}",

    // Init
    init_welcome: "🚀 VM Development Environment",
    init_already_exists: "⚠️  Configuration already exists",
    init_options_hint: "💡 Options:",
    init_success: "🎉 Ready to go!",
    init_next_steps: "Next steps:",

    // Temp VM
    temp_vm_status: "📊 Temp VM Status:",
    temp_vm_creating: "🚀 Creating temporary VM...",
    temp_vm_starting: "🚀 Starting temporary VM...",
    temp_vm_stopping: "🛑 Stopping temporary VM...",
    temp_vm_destroying: "🗑️ Destroying temporary VM...",
    temp_vm_destroyed: "✅ Temporary VM destroyed",
    temp_vm_failed_to_start: "❌ Failed to start temporary VM",
    temp_vm_connect_hint: "💡 Connect with: vm temp ssh",
    temp_vm_created_with_mounts: "✅ Temporary VM created with {count} mount(s)",
    temp_vm_connecting: "🔗 Connecting to temporary VM...",
    temp_vm_auto_destroying: "🗑️ Auto-destroying temporary VM...",
    temp_vm_usage_hint: "💡 Use 'vm temp ssh' to connect\n   Use 'vm temp destroy' when done",
    temp_vm_no_vm_found: "🔍 No temp VM found\n",
    temp_vm_create_hint: "💡 Create one with: vm temp create <directory>",
    temp_vm_container_info: "   Container: {name}",
    temp_vm_provider_info: "   Provider: {provider}",
    temp_vm_project_info: "   Project: {path}",
    temp_vm_mounts_info: "   Mounts: {count}",
    temp_vm_auto_destroy_enabled: "   Auto-destroy: enabled",
    temp_vm_stopped_success: "\n✅ Temporary VM stopped",
    temp_vm_restart_hint: "\n💡 Restart with: vm temp start",
    temp_vm_failed_to_stop: "\n❌ Failed to stop temporary VM",
    temp_vm_started_success: "\n✅ Temporary VM started",
    temp_vm_mounts_configured: "  Mounts:     {count} configured",
    temp_vm_restarting: "🔄 Restarting temporary VM...",
    temp_vm_stopping_step: "  ✓ Stopping container",
    temp_vm_starting_step: "  ✓ Starting container",
    temp_vm_services_ready: "  ✓ Services ready\n",
    temp_vm_restarted_success: "✅ Temporary VM restarted",
    temp_vm_mounts_active: "  Mounts:     {count} active",
    temp_vm_failed_to_restart: "\n❌ Failed to restart temporary VM",
    temp_vm_mount_added: "✅ Mount successfully applied",
    temp_vm_updating_container: "🔄 Updating container with new mount...",
    temp_vm_mount_applied: "\n✅ Mount successfully applied",
    temp_vm_mount_source: "  Source: {source}",
    temp_vm_mount_target: "  Target: {target}",
    temp_vm_mount_access: "  Access: {access}\n",
    temp_vm_view_mounts_hint: "💡 View all mounts: vm temp mounts",
    temp_vm_mounts_removed: "🗑️ Removed all {count} mount(s)",
    temp_vm_all_mounts_removed: "\n✅ All mounts removed ({count})",
    temp_vm_add_mounts_hint: "\n💡 Add new mounts: vm temp mount <source>:<target>",
    temp_vm_mount_removed: "\n✅ Mount removed",
    temp_vm_view_remaining_hint: "💡 View remaining mounts: vm temp mounts",
    temp_vm_unmount_required: "❌ Must specify what to unmount\n",
    temp_vm_unmount_options: "💡 Options:",
    temp_vm_unmount_specific: "  • Unmount specific: vm temp unmount --path <path>",
    temp_vm_unmount_all: "  • Unmount all: vm temp unmount --all",
    temp_vm_no_mounts: "📁 No mounts configured\n",
    temp_vm_add_mount_hint: "💡 Add a mount: vm temp mount <source>:<target>",
    temp_vm_current_mounts: "📁 Current mounts ({count})",
    temp_vm_mount_summary: "   {ro_count} read-only, {rw_count} read-write",
    temp_vm_list_header: "📋 Temp VMs:",
    temp_vm_list_item: "   {name} ({provider})",
    temp_vm_list_project: "      Project: {path}",
    temp_vm_list_mounts: "      Mounts: {count}",
    temp_vm_mount_removed_detail: "🗑️ Removed mount: {source} ({permissions})",
    temp_vm_mount_display_item: "   {source} → {target} ({permissions})",
    temp_vm_list_created_date: "      Created: {date}",
    temp_vm_list_empty: "📋 No temp VMs found\n",
    temp_vm_list_create_hint: "💡 Create one: vm temp create <directory>",
    temp_vm_confirm_add_mount: "Add mount {source} to temp VM? (y/N): ",
    temp_vm_confirm_remove_all_mounts: "Remove all {count} mounts from temp VM? (y/N): ",
    temp_vm_confirm_remove_mount: "Remove mount {source} from temp VM? (y/N): ",

    // Docker
    docker_is_running: "Docker is running.",
    docker_not_running: "Docker is not running. Please start it and try again.",
    docker_build_failed: "Docker build failed",
    docker_build_success: "Docker build successful",

    // Installer & Dependencies
    installer_checking_dependencies: "🔍 Checking dependencies...",
    installer_installing: "Installing VM Infrastructure...",
    installer_complete: "The 'vm' command is now available in new terminal sessions.",
    installer_help_hint: "For more information, run: vm --help",
    installer_path_already_configured: "✅ {path} is already in your PATH.",
    installer_path_not_configured: "⚠️ {path} is not in your PATH",
    installer_add_to_path_hint: "To add {path} to your PATH, add this line to your {profile}:",
    installer_manual_path_hint: "Or run: vm-package-manager link",

    // Package Management
    pkg_linking: "🔗 Package '{name}' is linked for {package_type}",
    pkg_linked_package: "🔗 Found linked local package: {name}",
    pkg_installing_local_cargo: "  -> Installing local cargo package from: {path}",
    pkg_linking_npm: "  -> Linking local npm package from: {path}",
    pkg_pipx_detected: "  -> Detected as a pipx environment",
    pkg_python_editable: "  -> Detected as a Python project, installing in editable mode",
    pkg_installing_editable: "  -> Installing as editable Python package",
    pkg_pipx_not_available: "  -> Pipx not available, using pip",
    pkg_no_bin_directory: "  -> No bin directory found in pipx environment",
    pkg_creating_wrappers: "  -> Creating wrapper scripts in {path}",
    pkg_wrapper_created: "    - Created wrapper: {name}",
    pkg_restart_shell: "  -> Please restart your shell to use them",
    pkg_no_linked_packages: "No linked packages found",
    pkg_linked_packages_header: "🔗 Linked packages:",

    // Provider Operations
    provider_tart_vm_exists: "⚠️  Tart VM '{name}' already exists.",
    provider_tart_recreate_hint: "To recreate, first run: vm destroy",
    provider_tart_created_success: "\n✅ Tart VM created successfully!",
    provider_tart_connect_hint: "💡 Use 'vm ssh' to connect to the VM",
    provider_tart_vm_created: "✅ Created Tart VM '{name}' from image '{image}'",
    provider_tart_vm_recreate_hint: "To recreate, first run: vm destroy {name}",
    provider_tart_vm_connect_hint: "💡 Use 'vm ssh {name}' to connect to the VM instance",
    provider_logs_unavailable: "The VM might not be running or logs may not be available yet.",
    provider_logs_expected_location: "Expected location: ~/.tart/vms/{name}/app.log",
    provider_logs_showing: "Showing Tart VM logs from: {path}",
    provider_vm_not_found: "VM '{name}' not found",
    provider_provisioning_unsupported: "Provisioning not supported for Tart VMs",
    provider_provisioning_explanation:
        "Tart VMs use pre-built images and don't support dynamic provisioning",

    // Audio
    audio_installing_pulseaudio: "🎧 Installing PulseAudio via Homebrew...",
    audio_stopping_services: "⏹️ Stopping audio services...",
    audio_starting_services: "🎧 Starting audio services...",

    // Ports
    ports_no_ranges: "No port ranges registered yet",
    ports_registered_ranges: "Registered port ranges:",
    ports_range_entry: "  {project}: {range} → {path}",

    // Progress Reporter
    progress_phase_header: "{icon} {phase}",
    progress_subtask: "{connector} {task}",
    progress_complete: "{connector} ✅ {message}",
    progress_warning: "{connector} ⚠️ {message}",
    progress_error: "{connector} ❌ {message}",
    progress_error_detail: "     └─ {detail}",
    progress_error_hint: "     💡 {hint}",

    // Status Formatter
    status_report_header: "VM Status Report",
    status_report_separator: "================",
    status_report_name: "Name: {name}",
    status_report_status: "Status: {status}",
    status_report_provider: "Provider: {provider}",
    status_report_memory: "Memory: {memory} MB",
    status_report_cpus: "CPUs: {cpus}",
};
