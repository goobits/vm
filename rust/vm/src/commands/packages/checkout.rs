use std::path::PathBuf;

use vm_config::config::VmConfig;
use vm_core::{vm_println, vm_progress, vm_success};
use vm_packages::{
    ApplianceState, CreateCheckout, PackageEcosystem, PackageInfrastructureClient,
    RegistryEndpoints, TransitionRequest, WorkflowState,
};

use crate::commands::command_context::{
    load_or_create_runtime_subject, project_name, RuntimeSubject,
};
use crate::error::{VmError, VmResult};

use super::{configured_state_and_client, files::ApplianceFiles, gateway_for_provider};

pub(super) struct CheckoutIntent {
    pub(super) config_path: Option<PathBuf>,
    pub(super) profile: Option<String>,
    pub(super) package: String,
    pub(super) agent: String,
    pub(super) consumer: Option<String>,
    pub(super) task: String,
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
            idempotency_key: uuid::Uuid::new_v4().to_string(),
        })
        .await?;
    let lease_token = checkout.lease_token.as_deref().ok_or_else(|| {
        VmError::validation(
            "Checkout was already created but its one-time lease token is unavailable",
            Some("Create a fresh checkout or use its existing assigned environment"),
        )
    })?;
    if let Err(error) = attach(
        &subject,
        &state,
        &checkout.checkout,
        lease_token,
        &consumer,
        definition.ecosystem,
    ) {
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
    vm_success!("Checkout {} is active", active.checkout_id);
    vm_println!(
        "Source: /tmp/vm-package-checkouts/{}/source",
        active.checkout_id
    );
    Ok(())
}

fn attach(
    subject: &RuntimeSubject,
    state: &ApplianceState,
    checkout: &vm_packages::CheckoutRecord,
    lease_token: &str,
    consumer: &str,
    ecosystem: PackageEcosystem,
) -> VmResult<()> {
    let root = format!("/tmp/vm-package-checkouts/{}", checkout.checkout_id);
    let source = format!("{root}/source");
    let archive = format!("/tmp/{}.bundle", checkout.checkout_id);
    let gateway = gateway_for_provider(state, subject.provider.name())?;
    let archive_client =
        PackageInfrastructureClient::new(RegistryEndpoints::new(gateway).map_err(VmError::from)?);
    let url = archive_client.checkout_archive_url(&checkout.checkout_id, consumer);
    let target = Some(subject.target.as_str());
    exec(subject, target, ["mkdir", "-p", root.as_str()])?;
    exec(
        subject,
        target,
        [
            "curl",
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--header",
            &format!("Authorization: Bearer {lease_token}"),
            &url,
            "--output",
            &archive,
        ],
    )?;
    exec(subject, target, ["git", "clone", &archive, source.as_str()])?;
    exec(
        subject,
        target,
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
    exec(subject, target, ["rm", "-f", archive.as_str()])?;
    exec(
        subject,
        target,
        [
            "/bin/sh",
            "-c",
            "umask 077; printf %s \"$1\" > \"$2\"",
            "vm-package-lease",
            lease_token,
            &format!("{root}/lease-token"),
        ],
    )?;
    apply_override(subject, target, ecosystem, &checkout.package, &source)
}

fn apply_override(
    subject: &RuntimeSubject,
    target: Option<&str>,
    ecosystem: PackageEcosystem,
    package: &str,
    source: &str,
) -> VmResult<()> {
    match ecosystem {
        PackageEcosystem::Npm => exec_in_workspace(
            subject,
            target,
            [
                "npm",
                "install",
                "--no-save",
                "--package-lock=false",
                source,
            ],
        ),
        PackageEcosystem::Python => exec_in_workspace(
            subject,
            target,
            ["python", "-m", "pip", "install", "--editable", source],
        ),
        PackageEcosystem::Cargo => {
            let patch = format!("patch.crates-io.{package}.path=\"{source}\"");
            exec_in_workspace(
                subject,
                target,
                [
                    "cargo",
                    "metadata",
                    "--format-version",
                    "1",
                    "--no-deps",
                    "--config",
                    &patch,
                ],
            )?;
            vm_println!("Cargo override: cargo --config '{patch}' <command>");
            Ok(())
        }
    }
}

fn exec_in_workspace<const N: usize>(
    subject: &RuntimeSubject,
    target: Option<&str>,
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
        .exec(target, &wrapped)
        .map_err(VmError::from)
}

fn exec<const N: usize>(
    subject: &RuntimeSubject,
    target: Option<&str>,
    command: [&str; N],
) -> VmResult<()> {
    let command = command.into_iter().map(str::to_string).collect::<Vec<_>>();
    subject
        .provider
        .exec(target, &command)
        .map_err(VmError::from)
}

fn workspace_path(config: &VmConfig) -> &str {
    config
        .project
        .as_ref()
        .and_then(|project| project.workspace_path.as_deref())
        .unwrap_or("/workspace")
}
