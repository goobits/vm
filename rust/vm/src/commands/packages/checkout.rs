use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use vm_core::{vm_hint, vm_println, vm_progress, vm_success};
use vm_packages::{
    CreateCheckout, PackageEcosystem, PackageInfrastructureClient, RegistryEndpoints,
    TransitionRequest, WorkflowState,
};

use crate::commands::command_context::{
    load_or_create_runtime_subject, load_runtime_subject, project_name,
};
use crate::error::{VmError, VmResult};

use super::{
    appliance::configured_state_and_client,
    files::ApplianceFiles,
    overrides::{cleanup_failed_attach, OverrideRecord},
    runtime::{
        checkout_root, copy_private, exec, gateway_for_provider, GuestRuntime, PackageExecutor,
    },
};

pub(super) struct CheckoutIntent {
    pub(super) config_path: Option<PathBuf>,
    pub(super) profile: Option<String>,
    pub(super) package: String,
    pub(super) agent: String,
    pub(super) consumer: Option<String>,
    pub(super) task: String,
}

pub(super) async fn cleanup_local(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    checkout: &vm_packages::CheckoutRecord,
) -> VmResult<()> {
    let subject = load_runtime_subject(config_path, profile, None)?;
    let root = checkout_root(&subject, &checkout.checkout_id)?;
    let current_project = project_name(&subject.config);
    if !checkout
        .consumers
        .iter()
        .any(|consumer| consumer == current_project)
    {
        return Err(VmError::validation(
            "Checkout is not assigned to the current project",
            None::<String>,
        ));
    }
    let record = OverrideRecord::load(&subject, &root, checkout, current_project)?;
    record.restore(&subject)?;
    exec(&subject, ["rm", "-rf", "--", root.as_str()])
}

pub(super) async fn handle(files: &ApplianceFiles, intent: CheckoutIntent) -> VmResult<()> {
    let subject = load_or_create_runtime_subject(intent.config_path, intent.profile, None).await?;
    let current_project = project_name(&subject.config).to_string();
    let consumer = intent.consumer.unwrap_or_else(|| current_project.clone());
    if consumer != current_project {
        return Err(VmError::validation(
            format!("Consumer '{consumer}' is not the current project '{current_project}'"),
            Some("Run this command from the selected consumer project"),
        ));
    }

    let (state, client) = configured_state_and_client(files)?;
    let definition = client.package_definition(&intent.package).await?;
    let pinned_version = client
        .package_consumers(&intent.package)
        .await?
        .iter()
        .find(|usage| usage.consumer == consumer)
        .map(|usage| usage.version.clone())
        .ok_or_else(|| {
            VmError::validation(
                format!(
                    "Consumer '{consumer}' has no registered '{}' dependency",
                    intent.package
                ),
                Some("Register the consumer and its pinned dependency before creating a checkout"),
            )
        })?;
    vm_progress!(
        "Preparing isolated '{}' checkout in package infrastructure...",
        intent.package
    );
    let checkout = client
        .create_checkout(&CreateCheckout {
            package: intent.package,
            agent: intent.agent.clone(),
            consumers: vec![consumer.clone()],
            task: intent.task,
            lease_token: vm_core::secrets::generate_random_password(48),
            idempotency_key: uuid::Uuid::new_v4().to_string(),
        })
        .await?;
    let lease_token = checkout.lease_token.as_deref().ok_or_else(|| {
        VmError::validation(
            "Checkout was already created but its one-time lease token is unavailable",
            Some("Create a fresh checkout or use its existing assigned environment"),
        )
    })?;
    let gateway = gateway_for_provider(&state, subject.provider.name())?;
    if let Err(error) = attach(
        &subject,
        &gateway,
        &checkout.checkout,
        lease_token,
        &consumer,
        definition.ecosystem,
        &pinned_version,
    ) {
        if let Ok(root) = checkout_root(&subject, &checkout.checkout.checkout_id) {
            if let Err(cleanup_error) = cleanup_failed_attach(&subject, &root) {
                vm_hint!("The incomplete override was retained at {root}: {cleanup_error}");
            }
        }
        let _ = client
            .transition(
                &checkout.checkout.checkout_id,
                &TransitionRequest {
                    next: WorkflowState::Failed,
                    actor: "vm-controller".into(),
                    reason: format!("consumer override failed: {error}"),
                    commit: checkout.checkout.base_commit.clone(),
                    validation_result: Some("failed".into()),
                    idempotency_key: format!("attach-failed-{}", checkout.checkout.checkout_id),
                },
            )
            .await;
        return Err(error);
    }
    let active = client
        .transition(
            &checkout.checkout.checkout_id,
            &TransitionRequest {
                next: WorkflowState::Active,
                actor: intent.agent,
                reason: format!("override attached to {consumer}"),
                commit: checkout.checkout.base_commit,
                validation_result: Some("override_ready".into()),
                idempotency_key: format!("active-{}", checkout.checkout.checkout_id),
            },
        )
        .await?;
    let root = checkout_root(&subject, &active.checkout_id)?;
    vm_success!("Checkout {} is active", active.checkout_id);
    vm_println!("Source: {root}/source");
    Ok(())
}

fn attach(
    subject: &impl PackageExecutor,
    gateway: &str,
    checkout: &vm_packages::CheckoutRecord,
    lease_token: &str,
    consumer: &str,
    ecosystem: PackageEcosystem,
    pinned_version: &str,
) -> VmResult<()> {
    let root = checkout_root(subject, &checkout.checkout_id)?;
    let source = format!("{root}/source");
    let archive = format!("/tmp/{}.bundle", checkout.checkout_id);
    let archive_client =
        PackageInfrastructureClient::new(RegistryEndpoints::new(gateway).map_err(VmError::from)?);
    let url = archive_client.checkout_archive_url(&checkout.checkout_id, consumer);
    let header = format!("{root}/authorization-header");
    exec(subject, ["mkdir", "-p", root.as_str()])?;
    copy_private(
        subject,
        format!("Authorization: Bearer {lease_token}\n").as_bytes(),
        &header,
    )?;
    exec(
        subject,
        [
            "curl",
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--header",
            &format!("@{header}"),
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
    let record = OverrideRecord::new(
        &checkout.checkout_id,
        consumer,
        &checkout.package,
        ecosystem,
        source,
        pinned_version,
    );
    record.write(subject, &root)?;
    if let Err(error) = record.activate(subject) {
        let _ = record.restore(subject);
        return Err(error);
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct GuestRequestState {
    lease_token: String,
    idempotency_key: String,
}

pub(super) async fn handle_guest(intent: CheckoutIntent) -> VmResult<()> {
    let subject = GuestRuntime::discover()?;
    let consumer = intent
        .consumer
        .unwrap_or_else(|| subject.consumer().to_string());
    if consumer != subject.consumer() {
        return Err(VmError::validation(
            "Package agent credential is bound to a different consumer",
            None::<String>,
        ));
    }
    let client = subject.client()?;
    let definition = client.package_definition(&intent.package).await?;
    let pinned_version = client
        .package_consumers(&intent.package)
        .await?
        .iter()
        .find(|usage| usage.consumer == consumer)
        .map(|usage| usage.version.clone())
        .ok_or_else(|| {
            VmError::validation(
                format!(
                    "Consumer '{consumer}' has no registered '{}' dependency",
                    intent.package
                ),
                Some("Register the consumer dependency before creating a checkout"),
            )
        })?;
    let intent_key = format!(
        "checkout-{}",
        &vm_packages::sha256_hex(format!(
            "{}\0{}\0{}\0{}",
            consumer, intent.package, intent.agent, intent.task
        ))[..32]
    );
    let request_path = subject.request_state_path(&intent_key)?;
    let request = read_or_create_request(&subject, &request_path, &intent_key)?;
    vm_progress!(
        "Preparing isolated '{}' checkout in package infrastructure...",
        intent.package
    );
    let checkout = client
        .create_checkout(&CreateCheckout {
            package: intent.package,
            agent: intent.agent.clone(),
            consumers: vec![consumer.clone()],
            task: intent.task,
            lease_token: request.lease_token.clone(),
            idempotency_key: request.idempotency_key,
        })
        .await?;
    if let Err(error) = attach(
        &subject,
        subject.gateway(),
        &checkout.checkout,
        &request.lease_token,
        &consumer,
        definition.ecosystem,
        &pinned_version,
    ) {
        if let Ok(root) = checkout_root(&subject, &checkout.checkout.checkout_id) {
            let _ = cleanup_failed_attach(&subject, &root);
        }
        return Err(error);
    }
    let active = client
        .transition(
            &checkout.checkout.checkout_id,
            &TransitionRequest {
                next: WorkflowState::Active,
                actor: intent.agent,
                reason: format!("override attached to {consumer}"),
                commit: checkout.checkout.base_commit,
                validation_result: Some("override_ready".into()),
                idempotency_key: format!("active-{}", checkout.checkout.checkout_id),
            },
        )
        .await?;
    let _ = std::fs::remove_file(request_path);
    let root = checkout_root(&subject, &active.checkout_id)?;
    vm_success!("Checkout {} is active", active.checkout_id);
    vm_println!("Source: {root}/source");
    Ok(())
}

fn read_or_create_request(
    subject: &GuestRuntime,
    path: &std::path::Path,
    idempotency_key: &str,
) -> VmResult<GuestRequestState> {
    match std::fs::read(path) {
        Ok(content) => serde_json::from_slice(&content).map_err(VmError::from),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let request = GuestRequestState {
                lease_token: vm_core::secrets::generate_random_password(48),
                idempotency_key: idempotency_key.to_string(),
            };
            let content = serde_json::to_vec(&request).map_err(VmError::from)?;
            subject.write_private(&content, &path.to_string_lossy())?;
            Ok(request)
        }
        Err(error) => Err(VmError::from(error)),
    }
}
