//! Common/shared messages across commands

pub struct CommonMessages {
    pub ports_label: &'static str,
    pub resources_label: &'static str,
    pub services_label: &'static str,
    pub error_generic: &'static str,
    pub press_ctrl_c_to_stop: &'static str,
}

pub const COMMON_MESSAGES: CommonMessages = CommonMessages {
    ports_label: "  Ports:      {start}-{end}",
    resources_label: "  Resources:  {cpus} CPUs, {memory}",
    services_label: "  Services:   {services}",
    error_generic: "❌ Error: {error}",
    press_ctrl_c_to_stop: "⏹️  Press Ctrl+C to stop...",
};
