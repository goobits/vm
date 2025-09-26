//! Central registry for all user-facing message templates.
//!
//! This file contains the `MESSAGES` constant, a comprehensive struct
//! that holds all static message strings used throughout the application.
//!
//! Templates use `{variable}` syntax for runtime values, which are
//! substituted by the `MessageBuilder`.

pub struct Messages {
    // Generic
    pub success: &'static str,
    pub failed: &'static str,
    pub error_generic: &'static str,
    pub warning_generic: &'static str,
    pub press_ctrl_c_to_stop: &'static str,

    // VM Operations
    pub vm_is_running: &'static str,
    pub vm_is_stopped: &'static str,
    pub vm_not_found: &'static str,
    pub vm_ambiguous: &'static str,
    pub vm_using: &'static str,

    // Config
    pub config_set_success: &'static str,
    pub config_apply_changes_hint: &'static str,
    pub config_available_presets: &'static str,

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

    // Docker
    pub docker_is_running: &'static str,
    pub docker_not_running: &'static str,
    pub docker_build_failed: &'static str,
    pub docker_build_success: &'static str,
}

pub const MESSAGES: Messages = Messages {
    // Generic
    success: "✅ Success",
    failed: "❌ Failed",
    error_generic: "Error: {error}",
    warning_generic: "Warning: {warning}",
    press_ctrl_c_to_stop: "Press Ctrl+C to stop...",

    // VM Operations
    vm_is_running: "✅ VM '{name}' is running",
    vm_is_stopped: "❌ VM '{name}' is stopped",
    vm_not_found: "No running VM found with that name.",
    vm_ambiguous: "\nMultiple VMs found with similar names:",
    vm_using: "Using: {name}",

    // Config
    config_set_success: "✅ Set {field} = {value} in {path}",
    config_apply_changes_hint: "💡 Apply changes: vm restart",
    config_available_presets: "📦 Available presets:",

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

    // Docker
    docker_is_running: "Docker is running.",
    docker_not_running: "Docker is not running. Please start it and try again.",
    docker_build_failed: "Docker build failed",
    docker_build_success: "Docker build successful",
};
