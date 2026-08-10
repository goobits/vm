use vm_config::config::VmConfig;

use crate::commands::command_context::RuntimeSubject;
use crate::error::{VmError, VmResult};

pub(super) fn checkout_root(checkout_id: &str) -> String {
    format!("/tmp/vm-package-checkouts/{checkout_id}")
}

pub(super) fn exec<const N: usize>(subject: &RuntimeSubject, command: [&str; N]) -> VmResult<()> {
    let command = command.into_iter().map(str::to_string).collect::<Vec<_>>();
    subject
        .provider
        .exec(Some(subject.target.as_str()), &command)
        .map_err(VmError::from)
}

pub(super) fn exec_in_workspace<const N: usize>(
    subject: &RuntimeSubject,
    command: [&str; N],
) -> VmResult<()> {
    let mut wrapped = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        "cd \"$1\"; shift; exec \"$@\"".to_string(),
        "vm-package-workspace".to_string(),
        workspace_path(&subject.config).to_string(),
    ];
    wrapped.extend(command.into_iter().map(str::to_string));
    subject
        .provider
        .exec(Some(subject.target.as_str()), &wrapped)
        .map_err(VmError::from)
}

fn workspace_path(config: &VmConfig) -> &str {
    config
        .project
        .as_ref()
        .and_then(|project| project.workspace_path.as_deref())
        .unwrap_or("/workspace")
}
