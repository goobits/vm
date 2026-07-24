//! Configuration-related messages (init, set, get, validate, presets, etc.)

pub struct ConfigMessages {
    pub ports_header: &'static str,
    pub ports_checking: &'static str,
    pub ports_fixing: &'static str,
    pub ports_resolved: &'static str,
    pub ports_updated: &'static str,
    pub ports_restart_hint: &'static str,

    pub set_success: &'static str,
    pub apply_changes_hint: &'static str,
    pub available_presets: &'static str,
    pub no_changes: &'static str,
    pub current_configuration: &'static str,
    pub modify_hint: &'static str,
    pub unset_success: &'static str,
    pub preset_applied: &'static str,
    pub restart_hint: &'static str,
    pub applied_presets: &'static str,
    pub apply_preset_hint: &'static str,
}

pub const CONFIG_MESSAGES: ConfigMessages = ConfigMessages {
    ports_header: "📡 Current port configuration:\n   Project: {project}\n   Port range: {range}",
    ports_checking: "🔍 Checking for port conflicts...",
    ports_fixing: "🔧 Fixing port conflicts...",
    ports_resolved: "\n✅ Port conflicts resolved\n\n  Old range:  {old}\n  New range:  {new}\n\n  ✓ Updated vm.yaml\n  ✓ Registered in port registry",
    ports_updated: "   📡 New port range: {range}",
    ports_restart_hint: "\n💡 Stop and run the environment again to apply changes",

    set_success: "✅ Set {field} = {value} in {path}",
    apply_changes_hint: "💡 Apply changes by stopping and running the environment again",
    available_presets: "📦 Available presets:",
    no_changes: "   ℹ️  (no changes were made to the file)",
    current_configuration: "📋 Current configuration\n",
    modify_hint: "💡 Modify with: vm config set <field> <value>",
    unset_success: "✅ Unset {field} in {path}",
    preset_applied: "✅ Applied preset '{preset}' to {path}",
    restart_hint: "\n💡 Stop and run the environment again to apply changes",
    applied_presets: "\n  Applied presets:",
    apply_preset_hint: "💡 Apply this preset: vm config preset {name}",
};
