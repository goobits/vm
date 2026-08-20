use vm_core::{vm_hint, vm_println, vm_progress, vm_success};
use vm_packages::{
    CleanupRequest, CreateCheckout, PackageEcosystem, PackageInfrastructureClient,
    RegistryEndpoints, SourceKind, ToolKind, TransitionRequest, WorkflowState,
};

use crate::error::{VmError, VmResult};

use super::{
    overrides::{cleanup_failed_attach, OverrideRecord},
    runtime::{checkout_root, exec, write_checkout_access, GuestRuntime},
};

const GUEST_WORK_TASK: &str = "managed guest package work";

#[derive(Clone, Copy)]
enum EditableSource {
    Package(PackageEcosystem),
    Tool(ToolKind),
}

impl EditableSource {
    fn kind(self) -> SourceKind {
        match self {
            Self::Package(_) => SourceKind::Package,
            Self::Tool(ToolKind::Binary) => SourceKind::ToolBinary,
            Self::Tool(ToolKind::Collection) => SourceKind::ToolCollection,
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
        if let Some(record) =
            OverrideRecord::load_optional(subject, &root, checkout, subject.consumer())?
        {
            record.restore(subject)?;
        }
    }
    exec(subject, ["rm", "-rf", "--", root.as_str()])
}

pub(super) async fn handle_guest(package: String) -> VmResult<()> {
    let subject = GuestRuntime::discover()?;
    let consumer = subject.consumer().to_string();
    let client = subject.client()?;
    let editable_source = editable_source(&client, &package).await?;
    let pinned_version = pinned_version(&client, &package, &consumer, editable_source).await?;
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
    let pinned_version = if checkout.checkout.source_only {
        None
    } else {
        pinned_version
    };
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
        subject.gateway(),
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
    if !checkout.consumers.contains(&subject.consumer().to_string()) {
        return Err(VmError::validation(
            "Checkout is not assigned to this managed environment",
            None::<String>,
        ));
    }
    let cancelled = if matches!(
        checkout.state,
        WorkflowState::Cancelled | WorkflowState::Closed
    ) {
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

fn attach(
    subject: &GuestRuntime,
    gateway: &str,
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
    let archive = format!("/tmp/{}.bundle", checkout.checkout_id);
    let archive_client =
        PackageInfrastructureClient::new(RegistryEndpoints::new(gateway).map_err(VmError::from)?);
    let url = archive_client.checkout_archive_url(&checkout.checkout_id, consumer);
    exec(subject, ["mkdir", "-p", root.as_str()])?;
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
    exec(subject, ["git", "clone", &archive, source.as_str()])?;
    exec(
        subject,
        [
            "git",
            "-C",
            source.as_str(),
            "switch",
            checkout
                .branch
                .as_deref()
                .ok_or_else(|| VmError::validation("Checkout branch is missing", None::<String>))?,
        ],
    )?;
    exec(subject, ["rm", "-f", archive.as_str()])?;
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
    if OverrideRecord::load_optional(subject, root, checkout, subject.consumer())?.is_some() {
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
    record.write(subject, root)?;
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

async fn editable_source(
    client: &PackageInfrastructureClient,
    name: &str,
) -> VmResult<EditableSource> {
    let (packages, tools) = tokio::try_join!(client.package_definitions(), client.tools())?;
    if let Some(package) = packages.iter().find(|package| package.name == name) {
        return Ok(EditableSource::Package(package.ecosystem));
    }
    match tools.iter().find(|tool| tool.name == name) {
        Some(tool) => Ok(EditableSource::Tool(tool.kind)),
        None => Err(VmError::validation(
            format!("No package or managed tool named '{name}' is registered"),
            Some("Run `vm packages doctor --fix` on the controller host"),
        )),
    }
}

async fn pinned_version(
    client: &PackageInfrastructureClient,
    package: &str,
    consumer: &str,
    source: EditableSource,
) -> VmResult<Option<String>> {
    let EditableSource::Package(_) = source else {
        return Ok(None);
    };
    Ok(consumer_version(
        &client.package_consumers(package).await?,
        consumer,
    ))
}

pub(super) fn consumer_version(
    usages: &[vm_packages::ConsumerUsage],
    consumer: &str,
) -> Option<String> {
    usages
        .iter()
        .find(|usage| usage.consumer == consumer)
        .map(|usage| usage.version.clone())
}

#[cfg(test)]
mod tests {
    use super::consumer_version;

    #[test]
    fn source_only_checkout_does_not_require_a_consumer_dependency() {
        let usages = [vm_packages::ConsumerUsage {
            consumer: "project-a".into(),
            version: "1.2.3".into(),
            pending_version: None,
            rollout_id: None,
        }];

        assert_eq!(
            consumer_version(&usages, "project-a").as_deref(),
            Some("1.2.3")
        );
        assert_eq!(consumer_version(&usages, "source-only-project"), None);
    }
}
