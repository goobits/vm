//! Service messages (auth proxy, package manager, docker, installer, provider, audio, temp VM, etc.)

pub struct ServiceMessages {
    // ============================================================================
    // Auth Proxy Messages
    // ============================================================================
    pub auth_secret_added: &'static str,
    pub auth_secrets_empty: &'static str,
    pub auth_secrets_list_header: &'static str,
    pub auth_secrets_show_values_hint: &'static str,
    pub auth_secret_removed: &'static str,
    pub auth_remove_cancelled: &'static str,
    pub auth_server_starting: &'static str,
    pub auth_server_started: &'static str,

    // ============================================================================
    // Docker Lifecycle Messages
    // ============================================================================
    pub docker_container_exists_prompt: &'static str,
    pub docker_container_exists_running: &'static str,
    pub docker_container_exists_stopped: &'static str,
    pub docker_container_choice_prompt: &'static str,
    pub docker_container_starting: &'static str,
    pub docker_container_recreating: &'static str,
    pub docker_ssh_info: &'static str,

    // Docker
    pub docker_is_running: &'static str,
    pub docker_not_running: &'static str,
    pub docker_build_failed: &'static str,
    pub docker_build_success: &'static str,

    // ============================================================================
    // Installer & Dependencies
    // ============================================================================
    pub installer_checking_dependencies: &'static str,
    pub installer_installing: &'static str,
    pub installer_complete: &'static str,
    pub installer_help_hint: &'static str,
    pub installer_path_already_configured: &'static str,
    pub installer_path_not_configured: &'static str,
    pub installer_add_to_path_hint: &'static str,
    pub installer_manual_path_hint: &'static str,
    pub installer_build_time_hint: &'static str,
    pub installer_sccache_enabled: &'static str,

    // ============================================================================
    // Package Management
    // ============================================================================
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
    pub pkg_manager_linked: &'static str,
    pub pkg_manager_not_linked: &'static str,

    // ============================================================================
    // Provider Operations
    // ============================================================================
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

    // ============================================================================
    // Audio
    // ============================================================================
    pub audio_installing_pulseaudio: &'static str,
    pub audio_stopping_services: &'static str,
    pub audio_starting_services: &'static str,

    // ============================================================================
    // Temp VM
    // ============================================================================
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

    // Init
    pub init_welcome: &'static str,
    pub init_already_exists: &'static str,
    pub init_options_hint: &'static str,
    pub init_success: &'static str,
    pub init_next_steps: &'static str,

    // ============================================================================
    // Progress/Configuration application Messages
    // ============================================================================
    pub progress_creating_vm: &'static str,
    pub progress_provisioning_complete: &'static str,
    pub progress_ansible_error: &'static str,

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

pub const SERVICE_MESSAGES: ServiceMessages = ServiceMessages {
    // Auth Proxy
    auth_secret_added: "✅ Secret '{name}' added successfully",
    auth_secrets_empty: "📭 No secrets stored\n\n💡 Add secrets with: vm secret add <name> <value>",
    auth_secrets_list_header: "🔐 Stored Secrets ({count})\n",
    auth_secrets_show_values_hint: "\n💡 Show values with: vm secret ls --show-values",
    auth_secret_removed: "✅ Secret '{name}' removed successfully",
    auth_remove_cancelled: "❌ Cancelled",
    auth_server_starting: "🚀 Starting auth proxy server...",
    auth_server_started: "✅ Auth proxy server started successfully",

    // Docker Lifecycle
    docker_container_exists_prompt: "\nWhat would you like to do?\n  1. {option1}\n  2. Recreate the container (remove and rebuild)\n  3. Cancel operation",
    docker_container_exists_running: "Keep using the existing running container",
    docker_container_exists_stopped: "Start the existing container",
    docker_container_choice_prompt: "\nChoice [1-3]: ",
    docker_container_starting: "\n▶️  Starting existing container...",
    docker_container_recreating: "\n🔄 Recreating container...",
    docker_ssh_info: "\n  User:  {user}\n  Path:  {path}\n  Shell: {shell}\n\n💡 Exit with: exit or Ctrl-D\n",

    // Docker
    docker_is_running: "✅ Docker is running.",
    docker_not_running: "❌ Docker is not running. Please start it and try again.",
    docker_build_failed: "❌ Docker build failed",
    docker_build_success: "✅ Docker build successful",

    // Installer & Dependencies
    installer_checking_dependencies: "🔍 Checking dependencies...",
    installer_installing: "📦 Installing VM Infrastructure...",
    installer_complete: "✅ The 'vm' command is now available in new terminal sessions.",
    installer_help_hint: "💡 For more information, run: vm --help",
    installer_path_already_configured: "✅ {path} is already in your PATH.",
    installer_path_not_configured: "⚠️ {path} is not in your PATH",
    installer_add_to_path_hint: "💡 To add {path} to your PATH, add this line to your {profile}:",
    installer_manual_path_hint: "💡 Or run: vm-package-manager link",
    installer_build_time_hint: "   This may take a few minutes on first build...",
    installer_sccache_enabled: "   Using sccache for faster builds",

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
    pkg_no_linked_packages: "📦 No linked packages found",
    pkg_linked_packages_header: "🔗 Linked packages:",
    pkg_manager_linked: "🔗 Package '{package}' is linked for {type}",
    pkg_manager_not_linked: "📦 Package '{package}' is not linked (would install from registry)",

    // Provider Operations
    provider_tart_vm_exists: "⚠️  Tart VM '{name}' already exists.",
    provider_tart_recreate_hint: "💡 To recreate, first run: vm rm",
    provider_tart_created_success: "\n✅ Tart VM created successfully!",
    provider_tart_connect_hint: "💡 Use 'vm shell' to connect to the VM",
    provider_tart_vm_created: "✅ Created Tart VM '{name}' from image '{image}'",
    provider_tart_vm_recreate_hint: "💡 To recreate, first run: vm rm {name}",
    provider_tart_vm_connect_hint: "💡 Use 'vm shell {name}' to connect to the VM instance",
    provider_logs_unavailable: "⚠️  The VM might not be running or logs may not be available yet.",
    provider_logs_expected_location: "💡 Expected location: ~/.tart/vms/{name}/app.log",
    provider_logs_showing: "📜 Showing Tart VM logs from: {path}",
    provider_vm_not_found: "❌ VM '{name}' not found",
    provider_provisioning_unsupported: "⚠️  Configuration application not supported for Tart VMs",
    provider_provisioning_explanation:
        "ℹ️  Tart VMs use pre-built images and don't support dynamic provisioning",

    // Audio
    audio_installing_pulseaudio: "🎧 Installing PulseAudio via Homebrew...",
    audio_stopping_services: "⏹️ Stopping audio services...",
    audio_starting_services: "🎧 Starting audio services...",

    // Temp VM
    temp_vm_status: "📊 Temp VM Status:",
    temp_vm_creating: "🚀 Creating temporary VM...",
    temp_vm_starting: "🚀 Starting temporary VM...",
    temp_vm_stopping: "🛑 Stopping temporary VM...",
    temp_vm_destroying: "🗑️ Removing temporary VM...",
    temp_vm_destroyed: "✅ Temporary VM removed",
    temp_vm_failed_to_start: "❌ Failed to start temporary VM",
    temp_vm_connect_hint: "💡 Connect with: vm shell <name>",
    temp_vm_created_with_mounts: "✅ Temporary VM created with {count} mount(s)",
    temp_vm_connecting: "🔗 Connecting to temporary VM...",
    temp_vm_auto_destroying: "🗑️ Auto-removing temporary VM...",
    temp_vm_usage_hint: "💡 Use 'vm shell <name>' to connect\n   Use 'vm rm <name>' when done",
    temp_vm_no_vm_found: "🔍 No temp VM found\n",
    temp_vm_create_hint: "💡 Create one with: vm run linux as <name> --ephemeral",
    temp_vm_container_info: "   Container: {name}",
    temp_vm_provider_info: "   Provider: {provider}",
    temp_vm_project_info: "   Project: {path}",
    temp_vm_mounts_info: "   Mounts: {count}",
    temp_vm_auto_destroy_enabled: "   Auto-destroy: enabled",
    temp_vm_stopped_success: "\n✅ Temporary VM stopped",
    temp_vm_restart_hint: "\n💡 Restart with: vm run linux as <name>",
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
    temp_vm_view_mounts_hint: "💡 View environments: vm ls",
    temp_vm_mounts_removed: "🗑️ Removed all {count} mount(s)",
    temp_vm_all_mounts_removed: "\n✅ All mounts removed ({count})",
    temp_vm_add_mounts_hint: "\n💡 Add mounts with: vm run linux as <name> --mount <source>:<target>",
    temp_vm_mount_removed: "\n✅ Mount removed",
    temp_vm_view_remaining_hint: "💡 View environments: vm ls",
    temp_vm_unmount_required: "❌ Must specify what to unmount\n",
    temp_vm_unmount_options: "💡 Options:",
    temp_vm_unmount_specific: "  • Re-run without that mount",
    temp_vm_unmount_all: "  • Remove environment: vm rm <name>",
    temp_vm_no_mounts: "📁 No mounts configured\n",
    temp_vm_add_mount_hint: "💡 Add a mount: vm run linux as <name> --mount <source>:<target>",
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
    temp_vm_list_create_hint: "💡 Create one: vm run linux as <name> --ephemeral",
    temp_vm_confirm_add_mount: "Add mount {source} to temp VM? (y/N): ",
    temp_vm_confirm_remove_all_mounts: "Remove all {count} mounts from temp VM? (y/N): ",
    temp_vm_confirm_remove_mount: "Remove mount {source} from temp VM? (y/N): ",

    // Init
    init_welcome: "🚀 VM Development Environment",
    init_already_exists: "⚠️  Configuration already exists",
    init_options_hint: "💡 Options:",
    init_success: "🎉 Ready to go!",
    init_next_steps: "Next steps:",

    // Progress/Configuration application
    progress_creating_vm: "Creating VM...",
    progress_provisioning_complete: "\n✅ Configuration application complete",
    progress_ansible_error: "\n❌ Error: {error}",

    // Ports
    ports_no_ranges: "📡 No port ranges registered yet",
    ports_registered_ranges: "📡 Registered port ranges:",
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
    status_report_header: "📊 VM Status Report",
    status_report_separator: "================",
    status_report_name: "Name: {name}",
    status_report_status: "Status: {status}",
    status_report_provider: "Provider: {provider}",
    status_report_memory: "Memory: {memory} MB",
    status_report_cpus: "CPUs: {cpus}",
};
