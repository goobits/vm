//! Fill absent effective VM fields from user-wide defaults.

use crate::config::{CpuLimit, MemoryLimit, VmConfig, VmSettings};
use crate::GlobalDefaults;

pub fn apply(config: &mut VmConfig, defaults: &GlobalDefaults) {
    if config.provider.is_none() {
        config.provider = defaults.provider.as_deref().map(Into::into);
    }
    if defaults.memory.is_some() || defaults.cpus.is_some() || defaults.user.is_some() {
        let vm = config.vm.get_or_insert_with(VmSettings::default);
        if vm.memory.is_none() {
            vm.memory = defaults.memory.map(MemoryLimit::Limited);
        }
        if vm.cpus.is_none() {
            vm.cpus = defaults.cpus.map(CpuLimit::Limited);
        }
        if vm.user.is_none() {
            vm.user = defaults.user.clone();
        }
    }
    if let Some(user) = &defaults.terminal {
        let terminal = config.terminal.get_or_insert_with(Default::default);
        if terminal.shell.is_none() {
            terminal.shell = user.shell.clone();
        }
        if terminal.theme.is_none() {
            terminal.theme = user.theme.clone();
        }
        if terminal.emoji.is_none() {
            terminal.emoji = user.emoji.clone();
        }
        if terminal.username.is_none() {
            terminal.username = user.username.clone();
        }
        terminal.show_git_branch = terminal.show_git_branch.or(user.show_git_branch);
        terminal.show_timestamp = terminal.show_timestamp.or(user.show_timestamp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ProviderName, TerminalConfig};

    #[test]
    fn user_defaults_fill_only_absent_fields() {
        let mut config = VmConfig {
            vm: Some(VmSettings {
                user: Some("project".into()),
                ..Default::default()
            }),
            terminal: Some(TerminalConfig {
                theme: Some("nord".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let defaults = GlobalDefaults {
            provider: Some("tart".into()),
            memory: Some(8192),
            cpus: Some(4),
            user: Some("user".into()),
            terminal: Some(TerminalConfig {
                theme: Some("dracula".into()),
                shell: Some("zsh".into()),
                ..Default::default()
            }),
        };
        apply(&mut config, &defaults);
        assert_eq!(config.provider, Some(ProviderName::Tart));
        assert_eq!(config.vm.as_ref().unwrap().user.as_deref(), Some("project"));
        assert_eq!(
            config.vm.as_ref().unwrap().memory,
            Some(MemoryLimit::Limited(8192))
        );
        assert_eq!(
            config.terminal.as_ref().unwrap().theme.as_deref(),
            Some("nord")
        );
        assert_eq!(
            config.terminal.as_ref().unwrap().shell.as_deref(),
            Some("zsh")
        );
    }
}
