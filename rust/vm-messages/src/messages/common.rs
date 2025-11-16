//! Common/shared messages across commands

pub struct CommonMessages {
    // ============================================================================
    // Common Messages (alphabetically sorted, shared across commands)
    // ============================================================================
    pub cleanup_complete: &'static str,
    pub configuring_services: &'static str,
    pub connect_hint: &'static str,
    pub ports_label: &'static str,
    pub resources_label: &'static str,
    pub services_cleaned: &'static str,
    pub services_cleanup_failed: &'static str,
    pub services_config_failed: &'static str,
    pub services_config_success: &'static str,
    pub services_label: &'static str,
    pub status_running: &'static str,
    pub status_stopped: &'static str,

    // ============================================================================
    // Error Messages (alphabetically sorted)
    // ============================================================================
    pub error_command_failed: &'static str,
    pub error_debug_info: &'static str,
    pub error_generic: &'static str,
    pub error_unexpected: &'static str,
    pub error_with_context: &'static str,

    // BoxSpec-related errors
    pub box_dockerfile_not_found: &'static str,
    pub box_provider_mismatch: &'static str,
    pub box_snapshot_use_restore: &'static str,

    // ============================================================================
    // Generic Messages (keeping for backwards compatibility)
    // ============================================================================
    pub failed: &'static str,
    pub press_ctrl_c_to_stop: &'static str,
    pub success: &'static str,
    pub warning_generic: &'static str,

    // ============================================================================
    // Common Validation Messages
    // ============================================================================
    pub validation_failed: &'static str,
    pub validation_hint: &'static str,
}

pub const COMMON_MESSAGES: CommonMessages = CommonMessages {
    // Common Messages
    cleanup_complete: "\n✅ Cleanup complete",
    configuring_services: "\n🔧 Configuring services...",
    connect_hint: "\n💡 Connect with: vm ssh",
    ports_label: "  Ports:      {start}-{end}",
    resources_label: "  Resources:  {cpus} CPUs, {memory}",
    services_cleaned: "  ✓ Services cleaned up successfully",
    services_cleanup_failed: "  ⚠️  Service cleanup failed: {error}",
    services_config_failed: "  Status:     ⚠️  Service configuration failed: {error}",
    services_config_success: "  Status:     ✅ Services configured successfully",
    services_label: "  Services:   {services}",
    status_running: "🟢 Running",
    status_stopped: "🔴 Stopped",

    // Error Messages
    error_command_failed: "❌ Command failed: {command}",
    error_debug_info: "🔍 Debug info: {details}",
    error_generic: "❌ Error: {error}",
    error_unexpected: "❌ Unexpected error occurred\n\n💡 Try: vm doctor",
    error_with_context: "{error}",

    // BoxSpec-related errors
    box_dockerfile_not_found: "Dockerfile not found at specified path. Please check the path in your vm.yaml configuration.",
    box_provider_mismatch: "The specified box type is not supported by the current provider. Docker/Podman support Dockerfiles and images, Tart uses OCI images.",
    box_snapshot_use_restore: "Snapshot reference detected in box field. To restore from a snapshot, use the command: vm snapshot restore <name>",

    // Generic Messages (keeping for backwards compatibility)
    failed: "❌ Failed!",
    press_ctrl_c_to_stop: "⏹️  Press Ctrl+C to stop...",
    success: "✅ Success!",
    warning_generic: "⚠️  Warning: {warning}",

    // Common Validation
    validation_failed: "❌ Configuration validation failed:",
    validation_hint: "\n💡 Fix the configuration errors above or run 'vm doctor' for more details",
};
