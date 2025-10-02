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
    pub common_services_config_failed: &'static str,
    pub common_services_config_success: &'static str,
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

    // ============================================================================
    // VM Stop Messages (alphabetically sorted)
    // ============================================================================
    pub vm_stop_force_header: &'static str,
    pub vm_stop_force_success: &'static str,
    pub vm_stop_force_troubleshooting: &'static str,
    pub vm_stop_header: &'static str,
    pub vm_stop_restart_hint: &'static str,
    pub vm_stop_success: &'static str,
    pub vm_stop_troubleshooting: &'static str,

    // ============================================================================
    // VM Restart Messages
    // ============================================================================
    pub vm_restart_header: &'static str,
    pub vm_restart_success: &'static str,
    pub vm_restart_troubleshooting: &'static str,

    // ============================================================================
    // VM Provision Messages
    // ============================================================================
    pub vm_provision_header: &'static str,
    pub vm_provision_hint: &'static str,
    pub vm_provision_progress: &'static str,
    pub vm_provision_success: &'static str,
    pub vm_provision_troubleshooting: &'static str,

    // ============================================================================
    // VM Exec Messages
    // ============================================================================
    pub vm_exec_header: &'static str,
    pub vm_exec_separator: &'static str,
    pub vm_exec_success: &'static str,
    pub vm_exec_failed: &'static str,
    pub vm_exec_troubleshooting: &'static str,

    // ============================================================================
    // VM Logs Messages
    // ============================================================================
    pub vm_logs_header: &'static str,
    pub vm_logs_separator: &'static str,
    pub vm_logs_footer: &'static str,
    pub vm_logs_troubleshooting: &'static str,

    // ============================================================================
    // VM List Messages
    // ============================================================================
    pub vm_list_empty: &'static str,
    pub vm_list_empty_provider: &'static str,
    pub vm_list_table_header: &'static str,
    pub vm_list_table_separator: &'static str,

    // ============================================================================
    // VM SSH Messages
    // ============================================================================
    pub vm_ssh_connecting: &'static str,
    pub vm_ssh_disconnected: &'static str,
    pub vm_ssh_vm_not_found: &'static str,
    pub vm_ssh_create_prompt: &'static str,
    pub vm_ssh_creating: &'static str,
    pub vm_ssh_create_success: &'static str,
    pub vm_ssh_create_failed: &'static str,
    pub vm_ssh_not_running: &'static str,
    pub vm_ssh_connection_lost: &'static str,
    pub vm_ssh_session_ended: &'static str,
    pub vm_ssh_start_hint: &'static str,
    pub vm_ssh_start_prompt: &'static str,
    pub vm_ssh_start_aborted: &'static str,
    pub vm_ssh_starting: &'static str,
    pub vm_ssh_start_failed: &'static str,
    pub vm_ssh_reconnecting: &'static str,

    // ============================================================================
    // VM Destroy Enhanced (Cross-Provider) Messages
    // ============================================================================
    pub vm_destroy_cross_no_instances: &'static str,
    pub vm_destroy_cross_list_header: &'static str,
    pub vm_destroy_cross_list_item: &'static str,
    pub vm_destroy_cross_confirm_prompt: &'static str,
    pub vm_destroy_cross_cancelled: &'static str,
    pub vm_destroy_cross_progress: &'static str,
    pub vm_destroy_cross_success_item: &'static str,
    pub vm_destroy_cross_failed: &'static str,
    pub vm_destroy_cross_complete: &'static str,

    // ============================================================================
    // Plugin Messages
    // ============================================================================
    pub plugin_list_empty: &'static str,
    pub plugin_list_header: &'static str,
    pub plugin_list_presets_header: &'static str,
    pub plugin_list_services_header: &'static str,
    pub plugin_list_item: &'static str,
    pub plugin_list_item_with_desc: &'static str,
    pub plugin_list_item_with_author: &'static str,
    pub plugin_info_preset_details_header: &'static str,
    pub plugin_info_service_details_header: &'static str,
    pub plugin_info_name: &'static str,
    pub plugin_info_version: &'static str,
    pub plugin_info_type: &'static str,
    pub plugin_info_description: &'static str,
    pub plugin_info_author: &'static str,
    pub plugin_info_content_file: &'static str,
    pub plugin_info_packages: &'static str,
    pub plugin_info_npm_packages: &'static str,
    pub plugin_info_pip_packages: &'static str,
    pub plugin_info_cargo_packages: &'static str,
    pub plugin_info_services: &'static str,
    pub plugin_info_image: &'static str,
    pub plugin_info_ports: &'static str,
    pub plugin_info_volumes: &'static str,
    pub plugin_install_validating: &'static str,
    pub plugin_install_validation_failed: &'static str,
    pub plugin_install_validation_error: &'static str,
    pub plugin_install_validation_error_with_suggestion: &'static str,
    pub plugin_install_warnings_header: &'static str,
    pub plugin_install_warnings: &'static str,
    pub plugin_install_warning_item: &'static str,
    pub plugin_install_success: &'static str,
    pub plugin_remove_success_preset: &'static str,
    pub plugin_remove_success_service: &'static str,
    pub plugin_validate_header: &'static str,
    pub plugin_validate_passed: &'static str,
    pub plugin_validate_warnings_header: &'static str,
    pub plugin_validate_ready: &'static str,
    pub plugin_validate_failed: &'static str,
    pub plugin_validate_errors_header: &'static str,
    pub plugin_validate_error_item: &'static str,
    pub plugin_validate_error_suggestion: &'static str,
    pub plugin_validate_warning_item: &'static str,
    pub plugin_new_success: &'static str,
    pub plugin_new_next_steps: &'static str,
    pub plugin_new_files_created: &'static str,

    // ============================================================================
    // Config Validation Messages
    // ============================================================================
    pub config_validate_header: &'static str,
    pub config_validate_valid: &'static str,
    pub config_validate_create_hint: &'static str,
    pub config_validate_invalid: &'static str,
    pub config_validate_fix_hint: &'static str,
    pub config_ports_header: &'static str,
    pub config_ports_checking: &'static str,
    pub config_ports_fixing: &'static str,
    pub config_ports_resolved: &'static str,
    pub config_ports_updated: &'static str,
    pub config_ports_restart_hint: &'static str,

    // ============================================================================
    // Config Error Messages
    // ============================================================================
    pub config_not_found: &'static str,
    pub config_not_found_hint: &'static str,

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

    // ============================================================================
    // VM Doctor Messages
    // ============================================================================
    pub vm_doctor_header: &'static str,
    pub vm_doctor_config_section: &'static str,
    pub vm_doctor_deps_section: &'static str,
    pub vm_doctor_services_section: &'static str,
    pub vm_doctor_summary_separator: &'static str,
    pub vm_doctor_all_passed: &'static str,
    pub vm_doctor_some_failed: &'static str,
    pub vm_doctor_config_loaded: &'static str,
    pub vm_doctor_config_valid: &'static str,
    pub vm_doctor_config_invalid: &'static str,
    pub vm_doctor_config_incomplete: &'static str,
    pub vm_doctor_config_complete: &'static str,
    pub vm_doctor_config_not_found: &'static str,
    pub vm_doctor_config_not_found_hint: &'static str,
    pub vm_doctor_config_load_failed: &'static str,
    pub vm_doctor_docker_found: &'static str,
    pub vm_doctor_docker_not_found: &'static str,
    pub vm_doctor_docker_not_found_hint: &'static str,
    pub vm_doctor_docker_check_failed: &'static str,
    pub vm_doctor_docker_daemon_running: &'static str,
    pub vm_doctor_docker_daemon_not_running: &'static str,
    pub vm_doctor_docker_daemon_not_running_hint: &'static str,
    pub vm_doctor_docker_daemon_check_failed: &'static str,
    pub vm_doctor_git_found: &'static str,
    pub vm_doctor_git_not_found: &'static str,
    pub vm_doctor_git_not_found_hint: &'static str,
    pub vm_doctor_git_check_failed: &'static str,
    pub vm_doctor_auth_healthy: &'static str,
    pub vm_doctor_auth_not_responding: &'static str,
    pub vm_doctor_auth_not_responding_hint: &'static str,
    pub vm_doctor_auth_check_failed: &'static str,
    pub vm_doctor_pkg_healthy: &'static str,
    pub vm_doctor_pkg_not_responding: &'static str,
    pub vm_doctor_pkg_not_responding_hint: &'static str,
    pub vm_doctor_pkg_check_failed: &'static str,
    pub vm_doctor_registry_healthy: &'static str,
    pub vm_doctor_registry_not_responding_active: &'static str,
    pub vm_doctor_registry_not_responding_hint: &'static str,
    pub vm_doctor_registry_not_running_info: &'static str,
    pub vm_doctor_registry_check_failed_active: &'static str,
    pub vm_doctor_registry_check_skipped: &'static str,

    // ============================================================================
    // VM Auth Proxy Messages
    // ============================================================================
    pub vm_auth_status_header: &'static str,
    pub vm_auth_reference_count: &'static str,
    pub vm_auth_registered_vms: &'static str,
    pub vm_auth_not_managed: &'static str,
    pub vm_auth_server_url: &'static str,
    pub vm_auth_health_ok: &'static str,
    pub vm_auth_health_failed: &'static str,
    pub vm_auth_auto_managed_info: &'static str,
    pub vm_auth_adding_secret: &'static str,
    pub vm_auth_secret_added: &'static str,
    pub vm_auth_removing_secret: &'static str,
    pub vm_auth_secret_removed: &'static str,
    pub vm_auth_interactive_header: &'static str,
    pub vm_auth_interactive_success: &'static str,

    // ============================================================================
    // VM Update Messages
    // ============================================================================
    pub vm_update_current_version: &'static str,
    pub vm_update_target_version: &'static str,
    pub vm_update_via_cargo: &'static str,
    pub vm_update_cargo_success: &'static str,
    pub vm_update_cargo_failed: &'static str,
    pub vm_update_downloading_github: &'static str,
    pub vm_update_fetching_release: &'static str,
    pub vm_update_release_fetch_failed: &'static str,
    pub vm_update_check_version_hint: &'static str,
    pub vm_update_platform_not_found: &'static str,
    pub vm_update_downloading_binary: &'static str,
    pub vm_update_download_failed: &'static str,
    pub vm_update_extracting: &'static str,
    pub vm_update_extract_failed: &'static str,
    pub vm_update_binary_not_found: &'static str,
    pub vm_update_backing_up: &'static str,
    pub vm_update_installing: &'static str,
    pub vm_update_success: &'static str,
    pub vm_update_new_version: &'static str,

    // ============================================================================
    // VM Dry Run Messages
    // ============================================================================
    pub vm_dry_run_header: &'static str,
    pub vm_dry_run_command: &'static str,
    pub vm_dry_run_config: &'static str,
    pub vm_dry_run_complete: &'static str,

    // ============================================================================
    // Common Validation Messages
    // ============================================================================
    pub common_validation_failed: &'static str,
    pub common_validation_hint: &'static str,

    // ============================================================================
    // Installer Messages
    // ============================================================================
    pub installer_build_time_hint: &'static str,
    pub installer_sccache_enabled: &'static str,

    // ============================================================================
    // Package Manager Messages
    // ============================================================================
    pub pkg_manager_linked: &'static str,
    pub pkg_manager_not_linked: &'static str,

    // VM Package Registry Messages
    pub vm_pkg_registry_status_header: &'static str,
    pub vm_pkg_registry_reference_count: &'static str,
    pub vm_pkg_registry_registered_vms: &'static str,
    pub vm_pkg_registry_not_managed: &'static str,
    pub vm_pkg_registry_health_ok: &'static str,
    pub vm_pkg_registry_health_failed: &'static str,
    pub vm_pkg_registry_auto_managed_info: &'static str,
    pub vm_pkg_publishing: &'static str,
    pub vm_pkg_removing: &'static str,
    pub vm_pkg_config_header: &'static str,
    pub vm_pkg_config_port: &'static str,
    pub vm_pkg_config_host: &'static str,
    pub vm_pkg_config_fallback: &'static str,
    pub vm_pkg_config_changes_hint: &'static str,
    pub vm_pkg_config_setting: &'static str,
    pub vm_pkg_use_bash_config: &'static str,
    pub vm_pkg_use_fish_config: &'static str,
    pub vm_pkg_use_unsupported: &'static str,
    pub vm_pkg_version_mismatch: &'static str,
    pub vm_pkg_restarting: &'static str,
    pub vm_pkg_server_starting: &'static str,
    pub vm_pkg_server_logs: &'static str,
    pub vm_pkg_server_started_info: &'static str,
    pub vm_pkg_serve_starting: &'static str,

    // VM Uninstall Messages
    pub vm_uninstall_header: &'static str,
    pub vm_uninstall_will_remove: &'static str,
    pub vm_uninstall_binary: &'static str,
    pub vm_uninstall_config_files: &'static str,
    pub vm_uninstall_config_file_item: &'static str,
    pub vm_uninstall_path_entries: &'static str,
    pub vm_uninstall_path_entry_item: &'static str,
    pub vm_uninstall_cancelled: &'static str,
    pub vm_uninstall_progress: &'static str,
    pub vm_uninstall_removing_file: &'static str,
    pub vm_uninstall_cleaned_path: &'static str,
    pub vm_uninstall_complete_instructions: &'static str,
    pub vm_uninstall_remove_cargo: &'static str,
    pub vm_uninstall_remove_sudo: &'static str,
    pub vm_uninstall_remove_no_sudo_hint: &'static str,
    pub vm_uninstall_remove_no_sudo: &'static str,
    pub vm_uninstall_remove_generic: &'static str,
    pub vm_uninstall_thank_you: &'static str,

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

    // ============================================================================
    // Progress/Provisioning Messages
    // ============================================================================
    pub progress_creating_vm: &'static str,
    pub progress_provisioning_complete: &'static str,
    pub progress_ansible_error: &'static str,

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
    common_services_config_failed: "  Status:     ⚠️  Service configuration failed: {error}",
    common_services_config_success: "  Status:     ✅ Services configured successfully",
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

    // ============================================================================
    // VM Stop Messages
    // ============================================================================
    vm_stop_force_header: "⚠️  Force stopping container '{name}'...",
    vm_stop_force_success: "✅ Container stopped\n\n🔧 Cleaning up services...",
    vm_stop_force_troubleshooting: "❌ Failed to stop container\n   Error: {error}",
    vm_stop_header: "🛑 Stopping '{name}'...",
    vm_stop_restart_hint: "\n💡 Restart with: vm start",
    vm_stop_success: "✅ Stopped successfully\n\n🔧 Cleaning up services...",
    vm_stop_troubleshooting: "❌ Failed to stop '{name}'\n   Error: {error}",

    // ============================================================================
    // VM Restart Messages
    // ============================================================================
    vm_restart_header: "🔄 Restarting '{name}'...",
    vm_restart_success: "✅ Restarted successfully",
    vm_restart_troubleshooting: "\n❌ Failed to restart '{name}'\n   Error: {error}",

    // ============================================================================
    // VM Provision Messages
    // ============================================================================
    vm_provision_header: "🔧 Re-provisioning '{name}'\n",
    vm_provision_hint: "\n💡 Changes applied to running container",
    vm_provision_progress: "  ✓ Updating packages\n  ✓ Installing dependencies\n  ✓ Configuring services\n  ✓ Restarting services",
    vm_provision_success: "\n✅ Provisioning complete",
    vm_provision_troubleshooting: "\n❌ Provisioning failed\n   Error: {error}\n\n💡 Check logs: vm logs",

    // ============================================================================
    // VM Exec Messages
    // ============================================================================
    vm_exec_header: "🏃 Running in '{name}': {command}\n──────────────────────────────────────────",
    vm_exec_separator: "──────────────────────────────────────────",
    vm_exec_success: "✅ Command completed successfully (exit code 0)\n\n💡 Run another: vm exec <command>",
    vm_exec_failed: "❌ Command failed\n   Error: {error}",
    vm_exec_troubleshooting: "❌ Command failed\n   Error: {error}\n\n💡 Debug with: vm ssh",

    // ============================================================================
    // VM Logs Messages
    // ============================================================================
    vm_logs_header: "📜 Logs for '{name}' (last 50 lines)\n──────────────────────────────────────────",
    vm_logs_separator: "──────────────────────────────────────────",
    vm_logs_footer: "──────────────────────────────────────────\n💡 Follow live: docker logs -f {container}\n💡 Full logs: docker logs {container}",
    vm_logs_troubleshooting: "❌ Failed to retrieve logs\n   Error: {error}",

    // ============================================================================
    // VM List Messages
    // ============================================================================
    vm_list_empty: "No VMs found",
    vm_list_empty_provider: "No VMs found for provider '{provider}'",
    vm_list_table_header: "INSTANCE             PROVIDER   STATUS       ID                   UPTIME     PROJECT        ",
    vm_list_table_separator: "─────────────────────────────────────────────────────────────────────────────────────────────────────",

    // ============================================================================
    // VM SSH Messages
    // ============================================================================
    vm_ssh_connecting: "🔗 Connecting to '{name}'...",
    vm_ssh_disconnected: "\n👋 Disconnected from '{name}'\n💡 Reconnect with: vm ssh",
    vm_ssh_vm_not_found: "\n🔍 VM '{name}' doesn't exist",
    vm_ssh_create_prompt: "\nWould you like to create it now? (y/N): ",
    vm_ssh_creating: "\n🚀 Creating '{name}'...\n\n  ✓ Building Docker image\n  ✓ Setting up volumes\n  ✓ Configuring network\n  ✓ Starting container\n  ✓ Running initial provisioning",
    vm_ssh_create_success: "\n✅ Created successfully\n\n🔗 Connecting to '{name}'...",
    vm_ssh_create_failed: "\n❌ Failed to create '{name}'\n   Error: {error}\n\n💡 Try:\n   • Check Docker: docker ps\n   • View logs: docker logs\n   • Manual create: vm create",
    vm_ssh_not_running: "\n⚠️  VM '{name}' is not running",
    vm_ssh_connection_lost: "\n⚠️  Lost connection to VM\n💡 Check if VM is running: vm status",
    vm_ssh_session_ended: "\n⚠️  Session ended unexpectedly\n💡 Check VM status: vm status",
    vm_ssh_start_hint: "\n💡 Start the VM with: vm start\n💡 Then reconnect with: vm ssh",
    vm_ssh_start_prompt: "\nWould you like to start it now? (y/N): ",
    vm_ssh_start_aborted: "\n❌ SSH connection aborted\n💡 Start the VM manually with: vm start",
    vm_ssh_starting: "\n🚀 Starting '{name}'...",
    vm_ssh_start_failed: "\n❌ Failed to start '{name}': {error}\n\n💡 Try:\n   • Check Docker status: docker ps\n   • View logs: docker logs {name}-dev\n   • Recreate VM: vm create --force",
    vm_ssh_reconnecting: "✅ Started successfully\n\n🔗 Reconnecting to '{name}'...",

    // ============================================================================
    // VM Destroy Enhanced (Cross-Provider) Messages
    // ============================================================================
    vm_destroy_cross_no_instances: "No instances found to destroy",
    vm_destroy_cross_list_header: "Instances to destroy:",
    vm_destroy_cross_list_item: "  {name} ({provider})",
    vm_destroy_cross_confirm_prompt: "\nAre you sure you want to destroy {count} instance(s)? (y/N): ",
    vm_destroy_cross_cancelled: "Destroy operation cancelled",
    vm_destroy_cross_progress: "Destroying {name} ({provider})...",
    vm_destroy_cross_success_item: "  ✅ Successfully destroyed {name}",
    vm_destroy_cross_failed: "  ❌ Failed to destroy {name}: {error}",
    vm_destroy_cross_complete: "\nDestroy operation completed:\n  Success: {success}\n  Errors: {errors}",

    // ============================================================================
    // Plugin Messages
    // ============================================================================
    plugin_list_empty: "No plugins installed.\n\nTo install a plugin:\n  vm plugin install <path-to-plugin>\n\nTo create a new plugin:\n  vm plugin new <plugin-name> --type <preset|service>",
    plugin_list_header: "Installed plugins:\n",
    plugin_list_presets_header: "Presets:",
    plugin_list_services_header: "Services:",
    plugin_list_item: "  {name} (v{version})",
    plugin_list_item_with_desc: "    {description}",
    plugin_list_item_with_author: "    Author: {author}",
    plugin_info_preset_details_header: "\nPreset Details:",
    plugin_info_service_details_header: "\nService Details:",
    plugin_info_name: "Plugin: {name}",
    plugin_info_version: "Version: {version}",
    plugin_info_type: "Type: {plugin_type}",
    plugin_info_description: "Description: {description}",
    plugin_info_author: "Author: {author}",
    plugin_info_content_file: "\nContent file: {file}",
    plugin_info_packages: "  Packages: {packages}",
    plugin_info_npm_packages: "  NPM Packages: {packages}",
    plugin_info_pip_packages: "  Pip Packages: {packages}",
    plugin_info_cargo_packages: "  Cargo Packages: {packages}",
    plugin_info_services: "  Services: {services}",
    plugin_info_image: "  Image: {image}",
    plugin_info_ports: "  Ports: {ports}",
    plugin_info_volumes: "  Volumes: {volumes}",
    plugin_install_validating: "Validating plugin...",
    plugin_install_validation_failed: "✗ Plugin validation failed:\n",
    plugin_install_validation_error: "  ✗ [{field}] {message}",
    plugin_install_validation_error_with_suggestion: "    → {suggestion}",
    plugin_install_warnings_header: "⚠ Warnings:",
    plugin_install_warnings: "⚠ Warnings:\n  {warnings}\n",
    plugin_install_warning_item: "  {warning}",
    plugin_install_success: "✓ Installed {type} plugin: {name} (v{version})",
    plugin_remove_success_preset: "✓ Removed preset plugin: {name}",
    plugin_remove_success_service: "✓ Removed service plugin: {name}",
    plugin_validate_header: "Validating plugin: {name}\n",
    plugin_validate_passed: "✓ Validation passed!\n",
    plugin_validate_warnings_header: "Warnings:",
    plugin_validate_ready: "Plugin '{name}' is valid and ready to use.",
    plugin_validate_failed: "✗ Validation failed!\n",
    plugin_validate_errors_header: "Errors:",
    plugin_validate_error_item: "  ✗ [{field}] {message}",
    plugin_validate_error_suggestion: "    → {suggestion}",
    plugin_validate_warning_item: "  ⚠ {warning}",
    plugin_new_success: "✓ Created {type} plugin template: {name}\n",
    plugin_new_next_steps: "Next steps:\n  1. cd {name}\n  2. Edit plugin.yaml to update metadata\n  3. Edit {type}.yaml to define your {type}\n  4. Test your plugin: vm plugin install .\n",
    plugin_new_files_created: "Files created:\n  - plugin.yaml: Plugin metadata\n  - {type}.yaml: {type_cap} configuration\n  - README.md: Plugin documentation",

    // ============================================================================
    // Config Validation Messages
    // ============================================================================
    config_validate_header: "🔍 Validating configuration...",
    config_validate_valid: "\n✅ Configuration is valid\n",
    config_validate_create_hint: "\n💡 Ready to create: vm create",
    config_validate_invalid: "\n❌ Configuration has errors\n",
    config_validate_fix_hint: "\n💡 Fix errors and try again",
    config_ports_header: "📡 Current port configuration:\n   Project: {project}\n   Port range: {range}",
    config_ports_checking: "🔍 Checking for port conflicts...",
    config_ports_fixing: "🔧 Fixing port conflicts...",
    config_ports_resolved: "\n✅ Port conflicts resolved\n\n  Old range:  {old}\n  New range:  {new}\n\n  ✓ Updated vm.yaml\n  ✓ Registered in port registry",
    config_ports_updated: "   📡 New port range: {range}",
    config_ports_restart_hint: "\n💡 Restart VM to apply: vm restart",

    // ============================================================================
    // Config Error Messages
    // ============================================================================
    config_not_found: "❌ No vm.yaml configuration file found\n",
    config_not_found_hint: "💡 You need a configuration file to run VMs. Try:\n   • Initialize config: vm init\n   • Change to project directory: cd <project>\n   • List existing VMs: vm list --all-providers",

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

    // ============================================================================
    // VM Doctor Messages
    // ============================================================================
    vm_doctor_header: "🩺 VM Environment Health Check\n==============================",
    vm_doctor_config_section: "📋 Configuration Validation:",
    vm_doctor_deps_section: "🔧 System Dependencies:",
    vm_doctor_services_section: "🔄 Background Services:",
    vm_doctor_summary_separator: "==============================",
    vm_doctor_all_passed: "All checks passed! Your VM environment is healthy.",
    vm_doctor_some_failed: "Some checks failed. Please review the issues above.",
    vm_doctor_config_loaded: "Configuration loaded successfully",
    vm_doctor_config_valid: "Configuration validation passed",
    vm_doctor_config_invalid: "Configuration validation failed:",
    vm_doctor_config_incomplete: "Configuration is incomplete (missing provider or project name)",
    vm_doctor_config_complete: "Configuration is complete",
    vm_doctor_config_not_found: "No vm.yaml configuration file found",
    vm_doctor_config_not_found_hint: "   💡 Run 'vm init' to create a configuration file",
    vm_doctor_config_load_failed: "Failed to load configuration: {error}",
    vm_doctor_docker_found: "Docker command found",
    vm_doctor_docker_not_found: "Docker command not found in PATH",
    vm_doctor_docker_not_found_hint: "   💡 Install Docker: https://docs.docker.com/get-docker/",
    vm_doctor_docker_check_failed: "Failed to check Docker: {error}",
    vm_doctor_docker_daemon_running: "Docker daemon is running",
    vm_doctor_docker_daemon_not_running: "Docker daemon is not running",
    vm_doctor_docker_daemon_not_running_hint: "   💡 Start Docker daemon or Docker Desktop",
    vm_doctor_docker_daemon_check_failed: "Failed to check Docker daemon: {error}",
    vm_doctor_git_found: "Git command found",
    vm_doctor_git_not_found: "Git command not found in PATH",
    vm_doctor_git_not_found_hint: "   💡 Install Git for version control support",
    vm_doctor_git_check_failed: "Failed to check Git: {error}",
    vm_doctor_auth_healthy: "Auth proxy service is healthy",
    vm_doctor_auth_not_responding: "Auth proxy service is not responding",
    vm_doctor_auth_not_responding_hint: "   💡 Start with: vm auth start",
    vm_doctor_auth_check_failed: "Failed to check auth proxy: {error}",
    vm_doctor_pkg_healthy: "Package server service is healthy",
    vm_doctor_pkg_not_responding: "Package server service is not responding",
    vm_doctor_pkg_not_responding_hint: "   💡 Start with: vm pkg start",
    vm_doctor_pkg_check_failed: "Failed to check package server: {error}",
    vm_doctor_registry_healthy: "Docker registry service is healthy",
    vm_doctor_registry_not_responding_active: "Docker registry service is not responding (needed for active VMs)",
    vm_doctor_registry_not_responding_hint: "   💡 Registry helps cache Docker images for faster VM operations",
    vm_doctor_registry_not_running_info: "   ℹ️  Docker registry service not running (not needed without active VMs)",
    vm_doctor_registry_check_failed_active: "Failed to check Docker registry: {error}",
    vm_doctor_registry_check_skipped: "   ℹ️  Docker registry check skipped (not needed without active VMs)",

    // ============================================================================
    // VM Auth Proxy Messages
    // ============================================================================
    vm_auth_status_header: "📊 Auth Proxy Status",
    vm_auth_reference_count: "   Reference Count: {count} VMs",
    vm_auth_registered_vms: "   Registered VMs:  {vms}",
    vm_auth_not_managed: "   Status: 🔴 Not managed by service manager",
    vm_auth_server_url: "   Server URL: {url}",
    vm_auth_health_ok: "   Health Check: ✅ Server responding",
    vm_auth_health_failed: "   Health Check: ❌ Server not responding",
    vm_auth_auto_managed_info: "\n💡 Service is automatically managed by VM lifecycle\n   • Auto-starts when VM with auth_proxy: true is created\n   • Auto-stops when last VM using it is destroyed",
    vm_auth_adding_secret: "🔐 Adding secret '{name}'...",
    vm_auth_secret_added: "Secret added successfully",
    vm_auth_removing_secret: "🗑️  Removing secret '{name}'...",
    vm_auth_secret_removed: "Secret removed successfully",
    vm_auth_interactive_header: "🔐 Interactive Secret Management\nThis will guide you through adding a new secret securely.",
    vm_auth_interactive_success: "Secret '{name}' added successfully",

    // ============================================================================
    // VM Update Messages
    // ============================================================================
    vm_update_current_version: "Current version: v{version}",
    vm_update_target_version: "Target version: {version}",
    vm_update_via_cargo: "Updating via cargo...",
    vm_update_cargo_success: "Successfully updated vm via cargo",
    vm_update_cargo_failed: "Failed to update: {error}",
    vm_update_downloading_github: "Downloading latest binary from GitHub releases...",
    vm_update_fetching_release: "Fetching release information...",
    vm_update_release_fetch_failed: "Failed to fetch release information",
    vm_update_check_version_hint: "Check if version '{version}' exists at {repo_url}/releases",
    vm_update_platform_not_found: "Could not find download URL for platform: {platform}",
    vm_update_downloading_binary: "Downloading vm binary...",
    vm_update_download_failed: "Failed to download binary",
    vm_update_extracting: "Extracting binary...",
    vm_update_extract_failed: "Failed to extract archive",
    vm_update_binary_not_found: "Binary not found in archive",
    vm_update_backing_up: "Backing up current binary...",
    vm_update_installing: "Installing new binary...",
    vm_update_success: "Successfully updated vm to {version}",
    vm_update_new_version: "New version: {version}",

    // ============================================================================
    // VM Dry Run Messages
    // ============================================================================
    vm_dry_run_header: "🔍 DRY RUN MODE - showing what would be executed:",
    vm_dry_run_command: "   Command: {command}",
    vm_dry_run_config: "   Config: {config}",
    vm_dry_run_complete: "🚫 Dry run complete - no commands were executed",

    // ============================================================================
    // Common Validation Messages
    // ============================================================================
    common_validation_failed: "Configuration validation failed:",
    common_validation_hint: "\n💡 Fix the configuration errors above or run 'vm doctor' for more details",

    // ============================================================================
    // Installer Messages
    // ============================================================================
    installer_build_time_hint: "   This may take a few minutes on first build...",
    installer_sccache_enabled: "   Using sccache for faster builds",

    // ============================================================================
    // Package Manager Messages
    // ============================================================================
    pkg_manager_linked: "🔗 Package '{package}' is linked for {type}",
    pkg_manager_not_linked: "📦 Package '{package}' is not linked (would install from registry)",

    // VM Package Registry Messages
    vm_pkg_registry_status_header: "📊 Package Registry Status",
    vm_pkg_registry_reference_count: "   Reference Count: {count} VMs",
    vm_pkg_registry_registered_vms: "   Registered VMs:  {vms}",
    vm_pkg_registry_not_managed: "   Status: 🔴 Not managed by service manager",
    vm_pkg_registry_health_ok: "   Health Check: ✅ Server responding",
    vm_pkg_registry_health_failed: "   Health Check: ❌ Server not responding",
    vm_pkg_registry_auto_managed_info: "\n💡 Service is automatically managed by VM lifecycle\n   • Auto-starts when VM with package_registry: true is created\n   • Auto-stops when last VM using it is destroyed",
    vm_pkg_publishing: "📦 Publishing package to local registry...",
    vm_pkg_removing: "🗑️  Removing package from registry...",
    vm_pkg_config_header: "Package Registry Configuration:",
    vm_pkg_config_port: "  Port: {port}",
    vm_pkg_config_host: "  Host: {host}",
    vm_pkg_config_fallback: "  Fallback: {fallback}",
    vm_pkg_config_changes_hint: "💡 Configuration changes will take effect on next server start",
    vm_pkg_config_setting: "Setting {key} = {value}",
    vm_pkg_use_bash_config: "# Package registry configuration for {shell}\nexport NPM_CONFIG_REGISTRY=http://localhost:{port}/npm/\nexport PIP_INDEX_URL=http://localhost:{port}/pypi/simple/\nexport PIP_TRUSTED_HOST=localhost\n\n# To apply: eval \"$(vm pkg use)\"",
    vm_pkg_use_fish_config: "# Package registry configuration for fish\nset -x NPM_CONFIG_REGISTRY http://localhost:{port}/npm/\nset -x PIP_INDEX_URL http://localhost:{port}/pypi/simple/\nset -x PIP_TRUSTED_HOST localhost",
    vm_pkg_use_unsupported: "Unsupported shell: {shell}\nSupported shells: bash, zsh, fish",
    vm_pkg_version_mismatch: "⚠️  Package server version mismatch: server={server_version}, cli={cli_version}",
    vm_pkg_restarting: "🔄 Restarting package server with new version...",
    vm_pkg_server_starting: "🚀 Starting package registry server...",
    vm_pkg_server_logs: "📝 Server logs: {log_path}",
    vm_pkg_server_started_info: "💡 Server is running as a detached background process\n   Access at: http://localhost:{port}",
    vm_pkg_serve_starting: "🚀 Starting package registry server...\n   Host: {host}\n   Port: {port}\n   Data: {data}",

    // VM Uninstall Messages
    vm_uninstall_header: "VM Uninstall\n============",
    vm_uninstall_will_remove: "\nThis will remove:",
    vm_uninstall_binary: "  • VM binary: {path}",
    vm_uninstall_config_files: "  • Configuration files:",
    vm_uninstall_config_file_item: "    - {path}",
    vm_uninstall_path_entries: "  • PATH entries in:",
    vm_uninstall_path_entry_item: "    - {path}",
    vm_uninstall_cancelled: "Uninstall cancelled.",
    vm_uninstall_progress: "\nUninstalling...",
    vm_uninstall_removing_file: "  Removing {path}",
    vm_uninstall_cleaned_path: "  Cleaned PATH from {path}",
    vm_uninstall_complete_instructions: "\nTo complete the uninstall, run:\n",
    vm_uninstall_remove_cargo: "  cargo uninstall vm",
    vm_uninstall_remove_sudo: "  sudo rm {path}",
    vm_uninstall_remove_no_sudo_hint: "\nOr without sudo if you have write permissions:",
    vm_uninstall_remove_no_sudo: "  rm {path}",
    vm_uninstall_remove_generic: "  rm {path}",
    vm_uninstall_thank_you: "\nThank you for using VM!",

    // ============================================================================
    // Auth Proxy Messages
    // ============================================================================
    auth_secret_added: "✅ Secret '{name}' added successfully",
    auth_secrets_empty: "📭 No secrets stored\n\n💡 Add secrets with: vm auth add <name> <value>",
    auth_secrets_list_header: "🔐 Stored Secrets ({count})\n",
    auth_secrets_show_values_hint: "\n💡 Show values with: vm auth list --show-values",
    auth_secret_removed: "✅ Secret '{name}' removed successfully",
    auth_remove_cancelled: "❌ Cancelled",
    auth_server_starting: "🚀 Starting auth proxy server...",
    auth_server_started: "✅ Auth proxy server started successfully",

    // ============================================================================
    // Docker Lifecycle Messages
    // ============================================================================
    docker_container_exists_prompt: "\nWhat would you like to do?\n  1. {option1}\n  2. Recreate the container (destroy and rebuild)\n  3. Cancel operation",
    docker_container_exists_running: "Keep using the existing running container",
    docker_container_exists_stopped: "Start the existing container",
    docker_container_choice_prompt: "\nChoice [1-3]: ",
    docker_container_starting: "\n▶️  Starting existing container...",
    docker_container_recreating: "\n🔄 Recreating container...",
    docker_ssh_info: "\n  User:  {user}\n  Path:  {path}\n  Shell: {shell}\n\n💡 Exit with: exit or Ctrl-D\n",

    // ============================================================================
    // Progress/Provisioning Messages
    // ============================================================================
    progress_creating_vm: "Creating VM...",
    progress_provisioning_complete: "\n✅ Provisioning complete",
    progress_ansible_error: "\n❌ Error: {error}",

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
