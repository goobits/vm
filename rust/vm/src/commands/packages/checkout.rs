use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use vm_core::{vm_hint, vm_println, vm_progress, vm_success};
use vm_packages::{
    CreateCheckout, PackageEcosystem, PackageInfrastructureClient, RegistryEndpoints, SourceKind,
    ToolKind, TransitionRequest, WorkflowState,
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

pub(super) struct ManagedCheckout {
    pub(super) subject: crate::commands::command_context::RuntimeSubject,
    pub(super) checkout: vm_packages::CheckoutRecord,
    pub(super) source: PathBuf,
}

#[derive(Clone, Copy)]
enum EditableSource {
    Package(PackageEcosystem),
    ToolCollection,
}

impl EditableSource {
    fn kind(self) -> SourceKind {
        match self {
            Self::Package(_) => SourceKind::Package,
            Self::ToolCollection => SourceKind::ToolCollection,
        }
    }
}

pub(super) async fn cleanup_local(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    checkout: &vm_packages::CheckoutRecord,
) -> VmResult<()> {
    let subject = load_runtime_subject(config_path, profile, None)?;
    cleanup_runtime(&subject, checkout, project_name(&subject.config))
}

pub(super) fn cleanup_guest(
    subject: &GuestRuntime,
    checkout: &vm_packages::CheckoutRecord,
) -> VmResult<()> {
    cleanup_runtime(subject, checkout, subject.consumer())
}

fn cleanup_runtime(
    subject: &impl PackageExecutor,
    checkout: &vm_packages::CheckoutRecord,
    current_project: &str,
) -> VmResult<()> {
    let root = checkout_root(subject, &checkout.checkout_id)?;
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
    if checkout.source_kind == SourceKind::Package {
        let record = OverrideRecord::load(subject, &root, checkout, current_project)?;
        record.restore(subject)?;
    }
    exec(subject, ["rm", "-rf", "--", root.as_str()])
}

pub(super) async fn handle(files: &ApplianceFiles, intent: CheckoutIntent) -> VmResult<()> {
    prepare(files, intent, false).await.map(|_| ())
}

pub(super) async fn prepare(
    files: &ApplianceFiles,
    intent: CheckoutIntent,
    resume: bool,
) -> VmResult<ManagedCheckout> {
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
    let source = editable_source(&client, &intent.package).await?;
    let pinned_version = pinned_version(&client, &intent.package, &consumer, source).await?;
    if resume {
        let matching = client
            .checkouts()
            .await?
            .into_iter()
            .filter(|checkout| {
                checkout.package == intent.package
                    && checkout.agent == intent.agent
                    && checkout.consumers.len() == 1
                    && checkout.consumers[0] == consumer
                    && checkout.task == intent.task
                    && matches!(
                        checkout.state,
                        WorkflowState::Created
                            | WorkflowState::CheckedOut
                            | WorkflowState::Active
                            | WorkflowState::Submitted
                            | WorkflowState::Validating
                            | WorkflowState::Reviewing
                            | WorkflowState::NeedsChanges
                            | WorkflowState::Approved
                            | WorkflowState::Integrating
                            | WorkflowState::ReadyToRelease
                            | WorkflowState::Publishing
                    )
            })
            .collect::<Vec<_>>();
        if matching.len() > 1 {
            return Err(VmError::validation(
                format!("{} matching managed checkouts are active", matching.len()),
                Some(format!(
                    "Run `vm packages cancel {}`",
                    matching[0].checkout_id
                )),
            ));
        }
        if let Some(checkout) = matching.into_iter().next() {
            let root = checkout_root(&subject, &checkout.checkout_id)?;
            let source = PathBuf::from(format!("{root}/source"));
            let source_path = source.to_string_lossy();
            if exec(&subject, ["test", "-d", source_path.as_ref()]).is_err() {
                return Err(VmError::validation(
                    format!(
                        "Managed checkout {} is active but its source directory is missing",
                        checkout.checkout_id
                    ),
                    Some(format!("Run `vm packages cancel {}`", checkout.checkout_id)),
                ));
            }
            vm_success!("Resuming checkout {}", checkout.checkout_id);
            return Ok(ManagedCheckout {
                subject,
                checkout,
                source,
            });
        }
    }
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
            workspace_release: false,
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
        source,
        pinned_version.as_deref(),
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
    Ok(ManagedCheckout {
        subject,
        checkout: active,
        source: PathBuf::from(format!("{root}/source")),
    })
}

fn attach(
    subject: &impl PackageExecutor,
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
    if let EditableSource::Package(ecosystem) = editable_source {
        let pinned_version = pinned_version.ok_or_else(|| {
            VmError::validation(
                "Package checkout is missing its pinned version",
                None::<String>,
            )
        })?;
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
    let source = editable_source(&client, &intent.package).await?;
    let pinned_version = pinned_version(&client, &intent.package, &consumer, source).await?;
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
            workspace_release: false,
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
        source,
        pinned_version.as_deref(),
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

async fn editable_source(
    client: &PackageInfrastructureClient,
    name: &str,
) -> VmResult<EditableSource> {
    let (packages, tools) = tokio::try_join!(client.package_definitions(), client.tools())?;
    if let Some(package) = packages.iter().find(|package| package.name == name) {
        return Ok(EditableSource::Package(package.ecosystem));
    }
    match tools.iter().find(|tool| tool.name == name) {
        Some(tool) if tool.kind == ToolKind::Collection => Ok(EditableSource::ToolCollection),
        Some(_) => Err(VmError::validation(
            format!("Tool '{name}' is not an editable collection"),
            None::<String>,
        )),
        None => Err(VmError::validation(
            format!("No package or tool collection named '{name}' is registered"),
            None::<String>,
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
    client
        .package_consumers(package)
        .await?
        .iter()
        .find(|usage| usage.consumer == consumer)
        .map(|usage| Some(usage.version.clone()))
        .ok_or_else(|| {
            VmError::validation(
                format!("Consumer '{consumer}' has no registered '{package}' dependency"),
                Some("Register the consumer and its pinned dependency before creating a checkout"),
            )
        })
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
