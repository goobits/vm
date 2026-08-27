use vm_core::{vm_hint, vm_println, vm_progress, vm_success};
use vm_packages::{
    CleanupRequest, CreateCheckout, PackageEcosystem, PackageInfrastructureClient, SourceKind,
    TransitionRequest, WorkflowState,
};

use crate::error::{VmError, VmResult};

use super::{
    guest_checkout::{
        checkout_root, create_directory, remove_directory, remove_file, write_checkout_access,
    },
    guest_runtime::{exec, exec_output, GuestRuntime},
    overrides::{cleanup_failed_attach, OverrideRecord},
};

const GUEST_WORK_TASK: &str = "managed guest package work";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditableSource {
    Package(PackageEcosystem),
    Tool(SourceKind),
}

impl EditableSource {
    fn kind(self) -> SourceKind {
        match self {
            Self::Package(_) => SourceKind::Package,
            Self::Tool(kind) => kind,
        }
    }
}

pub(super) fn cleanup_guest(
    subject: &GuestRuntime,
    checkout: &vm_packages::CheckoutRecord,
) -> VmResult<()> {
    let root = checkout_root(subject, &checkout.checkout_id)?;
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
    if checkout.source_kind == SourceKind::Package {
        if let Some(record) = OverrideRecord::load_optional(&root, checkout, subject.consumer())? {
            record.restore(subject)?;
        }
    }
    remove_directory(&root)
}

pub(super) fn cleanup_guest_after_release(
    subject: &GuestRuntime,
    checkout: &vm_packages::CheckoutRecord,
    released_commit: &str,
) -> VmResult<()> {
    let root = checkout_root(subject, &checkout.checkout_id)?;
    let source = format!("{root}/source");
    let status = exec_output(subject, ["git", "-C", &source, "status", "--porcelain"])?;
    let head = exec_output(subject, ["git", "-C", &source, "rev-parse", "HEAD"])?;
    if !checkout_is_disposable(&status, &head, released_commit) {
        return Err(VmError::validation(
            "Managed checkout contains work newer than the published release",
            Some("Preserve or move that work before removing the managed checkout"),
        ));
    }
    cleanup_guest(subject, checkout)
}

fn checkout_is_disposable(status: &str, head: &str, released_commit: &str) -> bool {
    status.trim().is_empty() && head.trim() == released_commit
}

pub(super) async fn handle_guest(package: String) -> VmResult<()> {
    let subject = GuestRuntime::discover()?;
    let consumer = subject.consumer().to_string();
    let client = subject.client()?;
    let lease_token = vm_core::secrets::generate_random_password(48);

    vm_progress!(
        "Preparing isolated '{}' checkout in package infrastructure...",
        package
    );
    let checkout = client
        .create_checkout(&CreateCheckout {
            package,
            // The authenticated workflow service replaces these compatibility
            // fields with its consumer-bound actor and stable purpose.
            agent: consumer.clone(),
            consumers: vec![consumer.clone()],
            task: GUEST_WORK_TASK.into(),
            workspace_release: false,
            // The workflow service derives this from the signed consumer identity.
            source_only: false,
            lease_token: lease_token.clone(),
            idempotency_key: uuid::Uuid::new_v4().to_string(),
        })
        .await?;
    let (editable_source, pinned_version) = editable_source(&checkout)?;
    let lease_token = checkout.lease_token.as_deref().ok_or_else(|| {
        VmError::validation(
            "Package infrastructure did not return a checkout lease",
            Some("Rerun `vm packages checkout` for the same source"),
        )
    })?;
    let root = checkout_root(&subject, &checkout.checkout.checkout_id)?;
    let source = format!("{root}/source");

    if std::path::Path::new(&source).is_dir() {
        refresh_checkout_access(&subject, &root, lease_token)?;
        ensure_override(
            &subject,
            &checkout.checkout,
            &root,
            &source,
            editable_source,
            pinned_version.as_deref(),
        )?;
        print_source(&checkout.checkout, &source, true);
        return Ok(());
    }
    if std::path::Path::new(&root).exists() {
        cleanup_failed_attach(&subject, &root)?;
    }
    attach(
        &subject,
        &client,
        &checkout.checkout,
        lease_token,
        &consumer,
        editable_source,
        pinned_version.as_deref(),
    )?;

    let active = if matches!(
        checkout.checkout.state,
        WorkflowState::Created | WorkflowState::CheckedOut
    ) {
        client
            .transition(
                &checkout.checkout.checkout_id,
                &TransitionRequest {
                    next: WorkflowState::Active,
                    actor: checkout.checkout.agent.clone(),
                    reason: format!("managed checkout attached to {consumer}"),
                    commit: checkout.checkout.base_commit,
                    validation_result: Some("checkout_ready".into()),
                    idempotency_key: format!("active-{}", checkout.checkout.checkout_id),
                },
            )
            .await?
    } else {
        checkout.checkout
    };
    print_source(&active, &source, false);
    Ok(())
}

pub(super) async fn cancel_guest() -> VmResult<()> {
    let subject = GuestRuntime::discover()?;
    let checkout_id = subject.current_checkout_id()?.ok_or_else(|| {
        VmError::validation(
            "Current directory is not inside a managed checkout",
            Some("Run `vm packages cancel` from the managed checkout source directory"),
        )
    })?;
    let client = subject.client()?;
    let checkout = client.checkout(&checkout_id).await?;
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
    let cancelled = if cleanup_ready(checkout.state) {
        checkout
    } else {
        client
            .transition(
                &checkout_id,
                &TransitionRequest {
                    next: WorkflowState::Cancelled,
                    actor: checkout.agent.clone(),
                    reason: "managed checkout cancelled by assigned guest".into(),
                    commit: None,
                    validation_result: Some("cancelled".into()),
                    idempotency_key: format!("cancel-{checkout_id}"),
                },
            )
            .await?
    };
    cleanup_guest(&subject, &cancelled)?;
    let closed = if cancelled.state == WorkflowState::Closed {
        cancelled
    } else {
        client
            .cleanup_checkout(
                &checkout_id,
                &CleanupRequest {
                    actor: cancelled.agent.clone(),
                    idempotency_key: format!("guest-cleanup-{checkout_id}"),
                },
            )
            .await?
    };
    vm_success!("Cancelled {}", closed.package);
    Ok(())
}

fn cleanup_ready(state: WorkflowState) -> bool {
    matches!(
        state,
        WorkflowState::Published
            | WorkflowState::Rejected
            | WorkflowState::Cancelled
            | WorkflowState::Failed
            | WorkflowState::Closed
    )
}

fn attach(
    subject: &GuestRuntime,
    client: &PackageInfrastructureClient,
    checkout: &vm_packages::CheckoutRecord,
    lease_token: &str,
    consumer: &str,
    editable_source: EditableSource,
    pinned_version: Option<&str>,
) -> VmResult<()> {
    if checkout.source_kind != editable_source.kind() {
        return Err(VmError::validation(
            "Checkout source kind does not match the registered source",
            None::<String>,
        ));
    }
    let root = checkout_root(subject, &checkout.checkout_id)?;
    let source = format!("{root}/source");
    let archive = format!("{root}/checkout.bundle");
    let branch = checkout
        .branch
        .as_deref()
        .ok_or_else(|| VmError::validation("Checkout branch is missing", None::<String>))?;
    let url = client.checkout_archive_url(&checkout.checkout_id, consumer);
    create_directory(&root)?;
    refresh_checkout_access(subject, &root, lease_token)?;
    exec(
        subject,
        [
            "curl",
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--header",
            &format!("@{root}/authorization-header"),
            &url,
            "--output",
            &archive,
        ],
    )?;
    exec(
        subject,
        [
            "git",
            "clone",
            "--branch",
            branch,
            &archive,
            source.as_str(),
        ],
    )?;
    remove_file(&archive)?;
    ensure_override(
        subject,
        checkout,
        &root,
        &source,
        editable_source,
        pinned_version,
    )
}

fn refresh_checkout_access(subject: &GuestRuntime, root: &str, lease_token: &str) -> VmResult<()> {
    write_checkout_access(subject, root, lease_token)
}

fn ensure_override(
    subject: &GuestRuntime,
    checkout: &vm_packages::CheckoutRecord,
    root: &str,
    source: &str,
    editable_source: EditableSource,
    pinned_version: Option<&str>,
) -> VmResult<()> {
    let (EditableSource::Package(ecosystem), Some(pinned_version)) =
        (editable_source, pinned_version)
    else {
        return Ok(());
    };
    if OverrideRecord::load_optional(root, checkout, subject.consumer())?.is_some() {
        return Ok(());
    }
    let record = OverrideRecord::new(
        &checkout.checkout_id,
        subject.consumer(),
        &checkout.package,
        ecosystem,
        source,
        pinned_version,
    );
    record.write(root)?;
    if let Err(error) = record.activate(subject) {
        let _ = record.restore(subject);
        return Err(error);
    }
    Ok(())
}

fn print_source(checkout: &vm_packages::CheckoutRecord, source: &str, resumed: bool) {
    if resumed {
        vm_success!("Resumed {}", checkout.package);
    } else {
        vm_success!("Checkout {} is active", checkout.checkout_id);
    }
    vm_println!("Source: {source}");
    vm_hint!("Continue with: cd {source}");
}

fn editable_source(
    checkout: &vm_packages::CheckoutLease,
) -> VmResult<(EditableSource, Option<String>)> {
    match checkout.checkout.source_kind {
        SourceKind::Package => {
            let context = checkout.package_context.as_ref().ok_or_else(|| {
                VmError::validation(
                    "Package infrastructure did not return package checkout context",
                    Some("Run `vm tools update` on the controller host, then retry the checkout"),
                )
            })?;
            let pinned_version = if checkout.checkout.source_only {
                None
            } else {
                Some(context.pinned_version.clone().ok_or_else(|| {
                    VmError::validation(
                        "Package infrastructure did not return the consumer's pinned version",
                        Some("Run `vm packages doctor --fix` on the controller host"),
                    )
                })?)
            };
            Ok((EditableSource::Package(context.ecosystem), pinned_version))
        }
        kind @ (SourceKind::ToolBinary | SourceKind::ToolCollection) => {
            Ok((EditableSource::Tool(kind), None))
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::{checkout_is_disposable, cleanup_ready, editable_source, EditableSource};
    use vm_packages::{
        CheckoutLease, CheckoutRecord, PackageCheckoutContext, PackageEcosystem, SourceKind,
        WorkflowState,
    };

    fn checkout(
        source_kind: SourceKind,
        source_only: bool,
        package_context: Option<PackageCheckoutContext>,
    ) -> CheckoutLease {
        let now = Utc::now();
        CheckoutLease {
            checkout: CheckoutRecord {
                checkout_id: "checkout-1".into(),
                package: "shared".into(),
                source_kind,
                agent: "project-a".into(),
                consumers: vec!["project-a".into()],
                task: "managed guest package work".into(),
                workspace_release: false,
                source_only,
                initial_release: false,
                state: WorkflowState::CheckedOut,
                base_branch: Some("main".into()),
                base_commit: Some("a".repeat(40)),
                branch: Some("agents/project-a/checkout-1".into()),
                worktree: None,
                lease: None,
                created_at: now,
                updated_at: now,
                transitions: Vec::new(),
            },
            lease_token: Some("lease-token".into()),
            package_context,
        }
    }

    #[test]
    fn rejected_checkout_can_be_cleaned_without_an_invalid_cancel_transition() {
        for state in [
            WorkflowState::Published,
            WorkflowState::Rejected,
            WorkflowState::Cancelled,
            WorkflowState::Failed,
            WorkflowState::Closed,
        ] {
            assert!(cleanup_ready(state));
        }
        assert!(!cleanup_ready(WorkflowState::Active));
        assert!(!cleanup_ready(WorkflowState::NeedsChanges));
    }

    #[test]
    fn package_checkout_context_preserves_override_inputs() {
        let assigned = checkout(
            SourceKind::Package,
            false,
            Some(PackageCheckoutContext {
                ecosystem: PackageEcosystem::Cargo,
                pinned_version: Some("1.2.3".into()),
            }),
        );
        let source_only = checkout(
            SourceKind::Package,
            true,
            Some(PackageCheckoutContext {
                ecosystem: PackageEcosystem::Python,
                pinned_version: None,
            }),
        );
        assert_eq!(
            editable_source(&assigned).unwrap(),
            (
                EditableSource::Package(PackageEcosystem::Cargo),
                Some("1.2.3".into())
            )
        );
        assert_eq!(
            editable_source(&source_only).unwrap(),
            (EditableSource::Package(PackageEcosystem::Python), None)
        );
    }

    #[test]
    fn tool_checkout_needs_no_package_context() {
        assert_eq!(
            editable_source(&checkout(SourceKind::ToolBinary, false, None)).unwrap(),
            (EditableSource::Tool(SourceKind::ToolBinary), None)
        );
        assert_eq!(
            editable_source(&checkout(SourceKind::ToolCollection, false, None)).unwrap(),
            (EditableSource::Tool(SourceKind::ToolCollection), None)
        );
    }

    #[test]
    fn published_checkout_cleanup_refuses_newer_or_uncommitted_work() {
        let released = "a".repeat(40);
        assert!(checkout_is_disposable("", &released, &released));
        assert!(!checkout_is_disposable(
            " M src/lib.rs\n",
            &released,
            &released
        ));
        assert!(!checkout_is_disposable("", &"b".repeat(40), &released));
    }
}
