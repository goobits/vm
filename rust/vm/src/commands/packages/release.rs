use vm_core::{vm_hint, vm_success};
use vm_packages::{LeaseRequest, WorkflowState};

use crate::error::{VmError, VmResult};

use super::{
    checkout, integration,
    runtime::{checkout_root, exec_output, GuestRuntime},
    submission,
};

const RELEASE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30 * 60);
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

pub(super) async fn handle_guest(checkout_id: Option<&str>) -> VmResult<()> {
    let subject = GuestRuntime::discover()?;
    let inferred_checkout_id;
    let checkout_id = match checkout_id {
        Some(checkout_id) => checkout_id,
        None => {
            inferred_checkout_id = infer_checkout_id(
                &std::env::current_dir().map_err(VmError::from)?,
                &dirs::home_dir().ok_or_else(|| {
                    VmError::validation("Guest home directory is unavailable", None::<String>)
                })?,
            )?;
            &inferred_checkout_id
        }
    };
    let client = subject.client()?;
    let checkout = client.checkout(checkout_id).await?;
    if !checkout.consumers.contains(&subject.consumer().to_string()) {
        return Err(VmError::validation(
            "Checkout is not assigned to this managed environment",
            None::<String>,
        ));
    }
    renew_release_lease(&subject, &client, &checkout).await?;
    let mut current = match checkout.state {
        WorkflowState::Active | WorkflowState::NeedsChanges => {
            submission::handle_guest(&subject, checkout_id).await?
        }
        WorkflowState::Submitted => submission::resume_guest(&subject, checkout_id).await?,
        WorkflowState::Validating
        | WorkflowState::Reviewing
        | WorkflowState::Approved
        | WorkflowState::Integrating
        | WorkflowState::ReadyToRelease
        | WorkflowState::Publishing
        | WorkflowState::Published
        | WorkflowState::Closed => client.checkout_submission(checkout_id).await?,
        state => {
            return Err(VmError::validation(
                format!("Checkout cannot be released from {state:?}"),
                Some("Inspect it with `vm packages show <checkout-id>`"),
            ))
        }
    };
    current = wait_for_review(&client, current).await?;
    if matches!(
        current.state,
        WorkflowState::Approved | WorkflowState::Integrating
    ) {
        current = integration::handle_guest(&subject, &current.submission_id).await?;
    }
    let published = wait_for_publication(&client, current).await?;
    let release_id = published.release_id.as_deref().ok_or_else(|| {
        VmError::validation("Published submission has no release record", None::<String>)
    })?;
    let release = client.release(release_id).await?;
    let checkout = client.checkout(checkout_id).await?;
    if let Err(error) = checkout::cleanup_guest(&subject, &checkout) {
        vm_hint!("Published successfully; local checkout cleanup was skipped: {error}");
    }
    vm_success!(
        "Released {}@{} from {}",
        release.package,
        release.version,
        release.source_commit
    );
    Ok(())
}

fn infer_checkout_id(current_dir: &std::path::Path, home: &std::path::Path) -> VmResult<String> {
    let root = home.join(".local/share/vm/package-checkouts");
    let relative = current_dir.strip_prefix(&root).map_err(|_| {
        VmError::validation(
            "Current directory is not inside a managed package checkout",
            Some("Run `vm packages release` from the managed checkout source directory"),
        )
    })?;
    let mut components = relative.components();
    let checkout_id = components
        .next()
        .and_then(|component| component.as_os_str().to_str())
        .ok_or_else(|| {
            VmError::validation(
                "Managed checkout path has no checkout identity",
                Some("Run `vm packages release` from the managed checkout source directory"),
            )
        })?;
    if components
        .next()
        .and_then(|component| component.as_os_str().to_str())
        != Some("source")
    {
        return Err(VmError::validation(
            "Current directory is not inside a managed checkout source directory",
            Some("Run `vm packages release` from the managed checkout source directory"),
        ));
    }
    vm_packages::validate_managed_id("checkout ID", checkout_id).map_err(VmError::from)?;
    Ok(checkout_id.to_string())
}

async fn renew_release_lease(
    subject: &GuestRuntime,
    client: &vm_packages::PackageInfrastructureClient,
    checkout: &vm_packages::CheckoutRecord,
) -> VmResult<()> {
    if checkout.state.revokes_lease() {
        return Ok(());
    }
    let root = checkout_root(subject, &checkout.checkout_id)?;
    let header = exec_output(subject, ["cat", &format!("{root}/authorization-header")])?;
    let token = header
        .trim()
        .strip_prefix("Authorization: Bearer ")
        .ok_or_else(|| {
            VmError::validation(
                "Managed checkout lease credential is invalid",
                None::<String>,
            )
        })?;
    client
        .renew_lease(
            &checkout.checkout_id,
            &LeaseRequest {
                holder: checkout.agent.clone(),
                lease_token: token.into(),
                duration_seconds: 24 * 60 * 60,
                idempotency_key: release_lease_key(checkout),
            },
        )
        .await?;
    Ok(())
}

fn release_lease_key(checkout: &vm_packages::CheckoutRecord) -> String {
    let generation = checkout.lease.as_ref().map_or_else(
        || checkout.updated_at.timestamp_millis(),
        |lease| lease.expires_at.timestamp_millis(),
    );
    format!("release-lease-{}-{generation}", checkout.checkout_id)
}

async fn wait_for_review(
    client: &vm_packages::PackageInfrastructureClient,
    mut submission: vm_packages::SubmissionRecord,
) -> VmResult<vm_packages::SubmissionRecord> {
    let deadline = tokio::time::Instant::now() + RELEASE_TIMEOUT;
    while matches!(
        submission.state,
        WorkflowState::Submitted | WorkflowState::Validating | WorkflowState::Reviewing
    ) {
        if tokio::time::Instant::now() >= deadline {
            return Err(VmError::validation(
                "Timed out waiting for package review",
                Some("Rerun the same release command to resume"),
            ));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
        submission = client.submission(&submission.submission_id).await?;
    }
    match submission.state {
        WorkflowState::NeedsChanges => Err(VmError::validation(
            submission
                .review
                .as_ref()
                .map(|review| format!("Package review requested changes: {}", review.reason))
                .unwrap_or_else(|| "Package review requested changes".into()),
            Some("Edit and commit the checkout, then rerun the same release command"),
        )),
        WorkflowState::Rejected | WorkflowState::Failed => Err(VmError::validation(
            "Package review rejected or failed the release",
            Some("Inspect the checkout and review receipt"),
        )),
        _ => Ok(submission),
    }
}

async fn wait_for_publication(
    client: &vm_packages::PackageInfrastructureClient,
    mut submission: vm_packages::SubmissionRecord,
) -> VmResult<vm_packages::SubmissionRecord> {
    let deadline = tokio::time::Instant::now() + RELEASE_TIMEOUT;
    while matches!(
        submission.state,
        WorkflowState::ReadyToRelease | WorkflowState::Publishing
    ) {
        if tokio::time::Instant::now() >= deadline {
            return Err(VmError::validation(
                "Timed out waiting for private package publication",
                Some("Rerun the same release command to resume"),
            ));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
        submission = client.submission(&submission.submission_id).await?;
    }
    match submission.state {
        WorkflowState::Published | WorkflowState::Closed => Ok(submission),
        WorkflowState::NeedsChanges => Err(VmError::validation(
            submission
                .review
                .as_ref()
                .map(|review| format!("Package release requested changes: {}", review.reason))
                .unwrap_or_else(|| "Package release requested changes".into()),
            Some("Edit and commit the checkout, then rerun the same release command"),
        )),
        state => Err(VmError::validation(
            format!("Package release stopped in {state:?}"),
            Some("Inspect package infrastructure logs and rerun when repaired"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::infer_checkout_id;
    use std::path::Path;

    #[test]
    fn checkout_identity_is_inferred_from_source_or_a_descendant() {
        let home = Path::new("/home/developer");
        assert_eq!(
            infer_checkout_id(
                Path::new("/home/developer/.local/share/vm/package-checkouts/checkout-123/source"),
                home,
            )
            .unwrap(),
            "checkout-123"
        );
        assert_eq!(
            infer_checkout_id(
                Path::new(
                    "/home/developer/.local/share/vm/package-checkouts/checkout-123/source/src"
                ),
                home,
            )
            .unwrap(),
            "checkout-123"
        );
        assert!(infer_checkout_id(Path::new("/workspace"), home).is_err());
    }
}
