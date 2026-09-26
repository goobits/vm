use std::sync::{Arc, Mutex};
use vm_core::{vm_hint, vm_println, vm_success, vm_warning};
use vm_packages::{
    LeaseRequest, PackageEcosystem, ReceiptKind, SourceKind, ToolActivationState,
    ToolActivationTargetState, ToolBuildPhase, WorkflowState,
};

use crate::error::{VmError, VmResult};

use super::{
    checkout,
    guest_checkout::{checkout_root, read_file},
    guest_runtime::GuestRuntime,
    integration, submission, workspace,
};

const RELEASE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30 * 60);
const ACTIVATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10 * 60);
const INITIAL_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);
const MAX_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);
const PROGRESS_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);

mod progress;
mod wait;

use progress::print_release_phase;
use wait::{
    checkout_package_ecosystem, renew_release_lease, submission_receipt, wait_for_publication,
    wait_for_review, wait_for_tool_activation,
};

pub(super) async fn handle_guest(receipt: Option<&str>, background: bool) -> VmResult<()> {
    let known_receipt = Arc::new(Mutex::new(receipt.map(str::to_owned)));
    tokio::select! {
        biased;
        signal = tokio::signal::ctrl_c() => {
            signal.map_err(|error| VmError::general(error, "Could not watch release interruption"))?;
            let known = known_receipt.lock().unwrap_or_else(|poison| poison.into_inner()).clone();
            Err(release_interruption(known.as_deref()))
        }
        result = handle_guest_inner(receipt, background, &known_receipt) => result,
    }
}

fn release_interruption(receipt: Option<&str>) -> VmError {
    match receipt {
        Some(receipt_id) => VmError::interrupted(
            format!("Stopped waiting for release receipt {receipt_id}; accepted work may continue"),
            Some(format!(
                "Inspect or resume with `vm packages release --receipt {receipt_id}`"
            )),
        ),
        None => VmError::interrupted(
            "Package release interrupted before a receipt was available",
            Some("Rerun `vm packages release` to inspect or resume the checkout"),
        ),
    }
}

async fn handle_guest_inner(
    receipt: Option<&str>,
    background: bool,
    known_receipt: &Arc<Mutex<Option<String>>>,
) -> VmResult<()> {
    let subject = GuestRuntime::discover()?;
    let client = subject.client()?;
    let receipt_checkout = if let Some(receipt_id) = receipt {
        vm_packages::validate_managed_id("workflow receipt", receipt_id).map_err(VmError::from)?;
        let record = client.receipt(receipt_id).await?;
        if !matches!(
            record.kind,
            ReceiptKind::Submission
                | ReceiptKind::Validation
                | ReceiptKind::Review
                | ReceiptKind::Integration
                | ReceiptKind::Build
                | ReceiptKind::Release
                | ReceiptKind::Publication
        ) {
            return Err(VmError::validation(
                format!("Receipt '{receipt_id}' does not identify a release"),
                None::<String>,
            ));
        }
        Some(record.checkout_id)
    } else {
        None
    };
    let mut workspace = None;
    let selected_checkout = match receipt_checkout {
        Some(checkout_id) => Some(checkout_id),
        None => subject.current_checkout_id()?,
    };
    let resolved_checkout_id = match selected_checkout {
        Some(checkout_id) => checkout_id,
        None => match workspace::prepare(&subject).await? {
            workspace::WorkspacePreparation::Published {
                release,
                source_kind,
            } => {
                vm_success!("Released {}@{}", release.package, release.version);
                if source_kind != SourceKind::Package {
                    let receipt_id = submission_receipt(&client, &release.checkout_id).await?;
                    *known_receipt
                        .lock()
                        .unwrap_or_else(|poison| poison.into_inner()) = Some(receipt_id.clone());
                    vm_println!("Receipt: {receipt_id}");
                    vm_hint!(
                        "Ctrl-C detaches; resume with `vm packages release --receipt {receipt_id}`"
                    );
                    wait_for_tool_activation(&client, &release.release_id, &receipt_id).await?;
                }
                return Ok(());
            }
            workspace::WorkspacePreparation::Pending(prepared) => {
                let checkout_id = prepared.checkout_id.clone();
                workspace = Some(prepared);
                checkout_id
            }
        },
    };
    let checkout_id = resolved_checkout_id.as_str();
    let checkout = client.checkout(checkout_id).await?;
    if !checkout
        .consumers
        .iter()
        .any(|consumer| consumer == subject.consumer())
    {
        return Err(VmError::validation(
            "Checkout is not assigned to this managed environment",
            None::<String>,
        ));
    }
    renew_release_lease(&subject, &client, &checkout).await?;
    let mut package_ecosystem = if matches!(
        checkout.state,
        WorkflowState::Active | WorkflowState::NeedsChanges | WorkflowState::Submitted
    ) || (checkout.state == WorkflowState::Created
        && checkout.workspace_release)
    {
        checkout_package_ecosystem(&client, &checkout).await?
    } else {
        None
    };
    let mut current = match checkout.state {
        WorkflowState::Created if checkout.workspace_release => {
            let workspace = workspace.as_ref().ok_or_else(|| {
                VmError::validation(
                    "Workspace checkout must be released from its canonical source directory",
                    Some("Run `vm packages release` from the canonical workspace"),
                )
            })?;
            submission::handle_workspace(
                &subject,
                &client,
                &checkout,
                &workspace.source,
                package_ecosystem,
            )
            .await?
        }
        WorkflowState::Active | WorkflowState::NeedsChanges => match workspace.as_ref() {
            Some(workspace) => {
                submission::handle_workspace(
                    &subject,
                    &client,
                    &checkout,
                    &workspace.source,
                    package_ecosystem,
                )
                .await?
            }
            None => {
                submission::handle_guest(&subject, &client, &checkout, package_ecosystem).await?
            }
        },
        WorkflowState::Submitted => match workspace.as_ref() {
            Some(workspace) => {
                submission::resume_workspace(
                    &subject,
                    &client,
                    &checkout,
                    &workspace.source,
                    package_ecosystem,
                )
                .await?
            }
            None => {
                submission::resume_guest(&subject, &client, &checkout, package_ecosystem).await?
            }
        },
        WorkflowState::Validating
        | WorkflowState::Reviewing
        | WorkflowState::Approved
        | WorkflowState::Integrating
        | WorkflowState::ReadyToRelease
        | WorkflowState::Publishing
        | WorkflowState::Published
        | WorkflowState::Closed => client.checkout_submission(checkout_id).await?,
        state => {
            return Err(VmError::conflict(
                format!("Checkout cannot be released from {state:?}"),
                Some("Inspect it with `vm packages checkout-show <checkout-id>`"),
            ))
        }
    };
    if let Some(workspace) = workspace.as_mut() {
        workspace.record_commit(&current.submitted_commit)?;
    }
    let receipt_id = submission_receipt(&client, checkout_id).await?;
    *known_receipt
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = Some(receipt_id.clone());
    vm_println!("Receipt: {receipt_id}");
    if background
        && matches!(
            current.state,
            WorkflowState::Submitted
                | WorkflowState::Validating
                | WorkflowState::Reviewing
                | WorkflowState::Approved
                | WorkflowState::Integrating
                | WorkflowState::ReadyToRelease
                | WorkflowState::Publishing
        )
    {
        vm_success!("Release accepted: {}", current.submission_id);
        vm_hint!(
            "Observe and complete guest integration with `vm packages release --receipt {}`",
            receipt_id
        );
        return Ok(());
    }
    if !matches!(
        current.state,
        WorkflowState::Published | WorkflowState::Closed
    ) {
        vm_println!("Release job: {}", current.submission_id);
        print_release_phase(&current);
        vm_hint!("Ctrl-C detaches without cancelling; resume with `vm packages release --receipt {receipt_id}`");
        if workspace.is_none() {
            vm_hint!("Cancel explicitly with `vm packages cancel` from this checkout");
        }
    } else if checkout.source_kind != SourceKind::Package {
        vm_hint!("Ctrl-C detaches; resume with `vm packages release --receipt {receipt_id}`");
    }
    current = wait_for_review(&client, current, checkout.workspace_release, &receipt_id).await?;
    if matches!(
        current.state,
        WorkflowState::Approved | WorkflowState::Integrating
    ) {
        vm_println!("Phase: integrating approved source");
        if package_ecosystem.is_none() {
            package_ecosystem = checkout_package_ecosystem(&client, &checkout).await?;
        }
        current =
            integration::handle_guest(&subject, &client, &checkout, &current, package_ecosystem)
                .await?;
        print_release_phase(&current);
    }
    let published =
        wait_for_publication(&client, current, checkout.workspace_release, &receipt_id).await?;
    let release_id = published.release_id.as_deref().ok_or_else(|| {
        VmError::operation("Published submission has no release record", None::<String>)
    })?;
    let release = client.release(release_id).await?;
    let managed_checkout = !checkout.workspace_release;
    if let Some(workspace) = workspace.as_mut() {
        workspace.record_commit(&release.source_commit)?;
    }
    vm_success!("Released {}@{}", release.package, release.version);
    if checkout.source_kind != SourceKind::Package {
        wait_for_tool_activation(&client, release_id, &receipt_id).await?;
    }
    if managed_checkout {
        if let Err(error) =
            checkout::cleanup_guest_after_release(&subject, &checkout, &published.submitted_commit)
        {
            vm_hint!("Published successfully; local checkout cleanup was skipped: {error}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
