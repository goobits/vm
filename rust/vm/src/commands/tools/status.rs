use std::collections::{BTreeMap, BTreeSet};

use vm_config::config::VmConfig;
use vm_core::{vm_hint, vm_println};
use vm_packages::WorkflowState;

use crate::error::VmResult;

use super::super::command_context::RuntimeSubject;
use super::super::{base, packages::tooling};
use super::guest::{self, InstalledTool};
use super::{
    command::yes_no,
    reconcile::{project_workspace, report_project_overrides},
};

#[derive(Debug, Clone, Default)]
struct ControllerToolState {
    registered: bool,
    published: bool,
    workflow: Option<ControllerWorkflow>,
}

#[derive(Debug, Clone)]
struct ControllerWorkflow {
    state: WorkflowState,
    submission_id: String,
    updated_at: chrono::DateTime<chrono::Utc>,
}

async fn controller_tool_states() -> VmResult<BTreeMap<String, ControllerToolState>> {
    let client = tooling::client()?;
    let definitions = client.tools().await?;
    let mut states = BTreeMap::new();
    for definition in definitions {
        let published = !client.tool(&definition.name).await?.artifacts.is_empty();
        states.insert(
            definition.name,
            ControllerToolState {
                registered: true,
                published,
                workflow: None,
            },
        );
    }
    for submission in client.submissions().await? {
        if !visible_workflow_state(submission.state) {
            continue;
        }
        let Some(state) = states.get_mut(&submission.package) else {
            continue;
        };
        if state
            .workflow
            .as_ref()
            .map_or(true, |current| submission.updated_at > current.updated_at)
        {
            state.workflow = Some(ControllerWorkflow {
                state: submission.state,
                submission_id: submission.submission_id,
                updated_at: submission.updated_at,
            });
        }
    }
    Ok(states)
}

pub(super) async fn show(subject: &RuntimeSubject) -> VmResult<()> {
    let target = guest::platform_target(subject.provider.as_ref(), &subject.target)?;
    let catalog = tooling::cached(&subject.config, &target)?;
    let project_overrides = match &catalog {
        Some(catalog) => guest::project_collection_overrides(
            subject.provider.as_ref(),
            &subject.target,
            project_workspace(&subject.config),
            &catalog.artifacts,
        )?,
        None => BTreeMap::new(),
    };
    let installed = guest::installed(subject.provider.as_ref(), &subject.target)?;
    let consumable = guest::consumable(subject.provider.as_ref(), &subject.target)?;
    let controller = match controller_tool_states().await {
        Ok(states) => Some(states),
        Err(error) => {
            vm_hint!("Controller package state is unavailable: {error}");
            None
        }
    };
    let vendor_tools = base::vendor_tool_statuses(subject.provider.as_ref(), &subject.target)?;

    vm_println!("Guest tools ({target})");
    vm_println!("NAME\tOWNER\tREGISTERED\tPUBLISHED\tINSTALLED\tCONSUMABLE\tPROJECT_COPY\tVERSION\tWORKFLOW\tJOB");
    for info in base::vendor_tool_info() {
        let state = &vendor_tools[info.name];
        if !base::vendor_tools_expected(&subject.config)
            && state.state == base::VendorToolState::Absent
        {
            continue;
        }
        vm_println!(
            "{}\tbase\tn/a\tn/a\t{}\t{}\tn/a\t{}\tn/a\tn/a",
            info.name,
            yes_no(state.state != base::VendorToolState::Absent),
            yes_no(state.state == base::VendorToolState::Consumable),
            state.version.as_deref().unwrap_or("-")
        );
    }
    for name in tool_status_names(
        &subject.config,
        controller.as_ref(),
        &installed,
        &consumable,
    ) {
        let controller_state = controller.as_ref().and_then(|states| states.get(&name));
        let installed_tool = installed.get(&name);
        let workflow = controller_state.and_then(|state| state.workflow.as_ref());
        vm_println!(
            "{}\tmanaged\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            name,
            controller_state.map_or("unknown", |state| yes_no(state.registered)),
            controller_state.map_or("unknown", |state| yes_no(state.published)),
            yes_no(installed_tool.is_some()),
            yes_no(consumable.get(&name).copied().unwrap_or(false)),
            if catalog.is_some() {
                yes_no(project_overrides.contains_key(&name))
            } else {
                "unknown"
            },
            installed_tool.map_or("-", |tool| tool.version.as_str()),
            workflow.map_or("-", |workflow| workflow_state_name(workflow.state)),
            workflow.map_or("-", |workflow| workflow.submission_id.as_str())
        );
    }
    report_project_overrides(&project_overrides);
    Ok(())
}

fn visible_workflow_state(state: WorkflowState) -> bool {
    matches!(
        state,
        WorkflowState::Submitted
            | WorkflowState::Validating
            | WorkflowState::Reviewing
            | WorkflowState::NeedsChanges
            | WorkflowState::Approved
            | WorkflowState::Integrating
            | WorkflowState::ReadyToRelease
            | WorkflowState::Publishing
    )
}

fn workflow_state_name(state: WorkflowState) -> &'static str {
    match state {
        WorkflowState::Submitted => "submitted",
        WorkflowState::Validating => "validating",
        WorkflowState::Reviewing => "reviewing",
        WorkflowState::NeedsChanges => "needs_changes",
        WorkflowState::Approved => "approved",
        WorkflowState::Integrating => "integrating",
        WorkflowState::ReadyToRelease => "ready_to_release",
        WorkflowState::Publishing => "publishing",
        _ => "-",
    }
}

fn tool_status_names(
    config: &VmConfig,
    controller: Option<&BTreeMap<String, ControllerToolState>>,
    installed: &BTreeMap<String, InstalledTool>,
    consumable: &BTreeMap<String, bool>,
) -> BTreeSet<String> {
    let mut names = config
        .tools
        .entries
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    if let Some(controller) = controller {
        names.extend(controller.keys().cloned());
    }
    names.extend(installed.keys().cloned());
    names.extend(consumable.keys().cloned());
    names.retain(|name| !base::is_vendor_tool(name));
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use vm_config::config::ToolConfig;

    #[test]
    fn status_includes_controller_and_stale_guest_state() {
        let mut config = VmConfig::default();
        config
            .tools
            .entries
            .insert("configured".into(), ToolConfig::default());
        let controller = BTreeMap::from([(
            "registered".into(),
            ControllerToolState {
                registered: true,
                published: false,
                workflow: None,
            },
        )]);
        let installed = BTreeMap::from([(
            "stale-installed".into(),
            InstalledTool {
                name: "stale-installed".into(),
                version: "1.0.0".into(),
                target: "linux-arm64".into(),
                digest: "a".repeat(64),
            },
        )]);
        let consumable = BTreeMap::from([("orphan-state".into(), false)]);

        assert_eq!(
            tool_status_names(&config, Some(&controller), &installed, &consumable)
                .into_iter()
                .collect::<Vec<_>>(),
            [
                "configured",
                "orphan-state",
                "registered",
                "stale-installed"
            ]
        );
    }

    #[test]
    fn workflow_status_exposes_in_flight_states_only() {
        assert!(visible_workflow_state(WorkflowState::ReadyToRelease));
        assert_eq!(
            workflow_state_name(WorkflowState::ReadyToRelease),
            "ready_to_release"
        );
        assert!(!visible_workflow_state(WorkflowState::Published));
        assert!(!visible_workflow_state(WorkflowState::Closed));
    }
}
