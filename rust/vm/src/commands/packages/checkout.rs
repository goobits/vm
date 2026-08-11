use std::path::PathBuf;

use vm_core::{vm_println, vm_progress, vm_success};
use vm_packages::{
    ApplianceState, CreateCheckout, PackageEcosystem, PackageInfrastructureClient,
    RegistryEndpoints, TransitionRequest, WorkflowState,
};

use crate::commands::command_context::{
    load_or_create_runtime_subject, load_runtime_subject, project_name, RuntimeSubject,
};
use crate::error::{VmError, VmResult};

use super::{
    appliance::configured_state_and_client,
    files::ApplianceFiles,
    runtime::{checkout_root, copy_private, exec, exec_in_workspace, gateway_for_provider},
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
    client: &PackageInfrastructureClient,
    checkout: &vm_packages::CheckoutRecord,
) -> VmResult<()> {
    let subject = load_runtime_subject(config_path, profile, None)?;
    let root = checkout_root(&checkout.checkout_id)?;
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
    let definition = client.package_definition(&checkout.package).await?;
    let pinned_version = client
        .package_consumers(&checkout.package)
        .await?
        .into_iter()
        .find(|usage| usage.consumer == current_project)
        .map(|usage| usage.version);
    restore_published_override(
        &subject,
        definition.ecosystem,
        &checkout.package,
        pinned_version.as_deref(),
    )?;
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
    if !client
        .package_consumers(&intent.package)
        .await?
        .iter()
        .any(|usage| usage.consumer == consumer)
    {
        return Err(VmError::validation(
            format!(
                "Consumer '{consumer}' has no registered '{}' dependency",
                intent.package
            ),
            Some("Register the consumer and its pinned dependency before creating a checkout"),
        ));
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
        if let Ok(root) = checkout_root(&checkout.checkout.checkout_id) {
            let _ = exec(&subject, ["rm", "-rf", "--", root.as_str()]);
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
    let root = checkout_root(&checkout.checkout_id)?;
    let source = format!("{root}/source");
    let archive = format!("/tmp/{}.bundle", checkout.checkout_id);
    let gateway = gateway_for_provider(state, subject.provider.name())?;
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
    apply_override(subject, ecosystem, &checkout.package, &source)
}

fn apply_override(
    subject: &RuntimeSubject,
    ecosystem: PackageEcosystem,
    package: &str,
    source: &str,
) -> VmResult<()> {
    let command = override_command(ecosystem, package, DependencySource::Worktree(source));
    exec_in_workspace(subject, command)?;
    if ecosystem == PackageEcosystem::Cargo {
        let patch = cargo_patch(package, source);
        vm_println!("Cargo override: cargo --config '{patch}' <command>");
    }
    Ok(())
}

fn restore_published_override(
    subject: &RuntimeSubject,
    ecosystem: PackageEcosystem,
    package: &str,
    pinned_version: Option<&str>,
) -> VmResult<()> {
    let Some(version) = pinned_version else {
        if ecosystem == PackageEcosystem::Cargo {
            return Ok(());
        }
        return Err(VmError::validation(
            format!("No registered version is available to restore for '{package}'"),
            Some("The temporary checkout was retained to avoid breaking the consumer"),
        ));
    };
    if ecosystem == PackageEcosystem::Cargo {
        return Ok(());
    }
    exec_in_workspace(
        subject,
        override_command(ecosystem, package, DependencySource::Published(version)),
    )
}

#[derive(Clone, Copy)]
enum DependencySource<'a> {
    Worktree(&'a str),
    Published(&'a str),
}

fn override_command(
    ecosystem: PackageEcosystem,
    package: &str,
    source: DependencySource<'_>,
) -> Vec<String> {
    match (ecosystem, source) {
        (PackageEcosystem::Npm, DependencySource::Worktree(path)) => {
            ["npm", "install", "--no-save", "--package-lock=false", path]
                .into_iter()
                .map(str::to_string)
                .collect()
        }
        (PackageEcosystem::Npm, DependencySource::Published(version)) => vec![
            "npm".into(),
            "install".into(),
            "--no-save".into(),
            "--package-lock=false".into(),
            format!("{package}@{version}"),
        ],
        (PackageEcosystem::Python, DependencySource::Worktree(path)) => {
            ["python", "-m", "pip", "install", "--editable", path]
                .into_iter()
                .map(str::to_string)
                .collect()
        }
        (PackageEcosystem::Python, DependencySource::Published(version)) => vec![
            "python".into(),
            "-m".into(),
            "pip".into(),
            "install".into(),
            "--force-reinstall".into(),
            "--no-deps".into(),
            format!("{package}=={version}"),
        ],
        (PackageEcosystem::Cargo, DependencySource::Worktree(path)) => vec![
            "cargo".into(),
            "metadata".into(),
            "--format-version".into(),
            "1".into(),
            "--no-deps".into(),
            "--config".into(),
            cargo_patch(package, path),
        ],
        (PackageEcosystem::Cargo, DependencySource::Published(_)) => Vec::new(),
    }
}

fn cargo_patch(package: &str, source: &str) -> String {
    format!("patch.crates-io.{package}.path=\"{source}\"")
}

#[cfg(test)]
mod tests {
    use super::{override_command, DependencySource};
    use vm_packages::PackageEcosystem;

    #[test]
    fn ecosystem_overrides_share_one_adapter() {
        assert_eq!(
            override_command(
                PackageEcosystem::Npm,
                "@internal/auth",
                DependencySource::Published("1.4.2")
            ),
            [
                "npm",
                "install",
                "--no-save",
                "--package-lock=false",
                "@internal/auth@1.4.2"
            ]
        );
        assert_eq!(
            override_command(
                PackageEcosystem::Python,
                "internal-auth",
                DependencySource::Worktree("/tmp/auth")
            ),
            ["python", "-m", "pip", "install", "--editable", "/tmp/auth"]
        );
        assert!(override_command(
            PackageEcosystem::Cargo,
            "auth",
            DependencySource::Worktree("/tmp/auth")
        )
        .ends_with(&[
            "--config".to_string(),
            "patch.crates-io.auth.path=\"/tmp/auth\"".to_string()
        ]));
        assert!(override_command(
            PackageEcosystem::Cargo,
            "auth",
            DependencySource::Published("1.4.2")
        )
        .is_empty());
    }
}
