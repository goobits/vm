use std::path::Path;
use std::process::{Command, ExitCode};

use anyhow::{anyhow, bail, Context, Error, Result};
use vm_logging::init_service_subscriber;
use vm_package_jobs::runtime::{
    authorization_header, command_text as text, download_bundle, operation_key,
    required_secret as secret, run_command as run, worker_main, JobMonitor, QueueMonitor,
    POLL_INTERVAL,
};
use vm_packages::{
    PackageEcosystem, PackageInfrastructureClient, RegistryEndpoints, RolloutState,
    RolloutValidationRequest,
};

#[tokio::main]
async fn main() -> ExitCode {
    let _guard = init_service_subscriber();
    worker_main("package_rollout", run_worker()).await
}

async fn run_worker() -> Result<()> {
    let gateway =
        std::env::var("PKG_ROLLOUT_GATEWAY").unwrap_or_else(|_| "http://gateway:8080".into());
    let token = secret("PKG_ROLLOUT_TOKEN_FILE")?;
    let client = PackageInfrastructureClient::new(RegistryEndpoints::new(gateway)?)
        .with_rollout_token(token.clone());
    let mut queue = QueueMonitor::new("poll_rollout_queue");
    let mut jobs = JobMonitor::new("rollout");
    loop {
        let delay = match client.reconcile_rollout_queue().await {
            Ok(Some(rollout)) => {
                queue.available();
                match run_rollout(&client, &token, &rollout.rollout_id).await {
                    Ok(()) => {
                        jobs.succeeded(&rollout.rollout_id);
                        POLL_INTERVAL
                    }
                    Err(error) => jobs.failed(&rollout.rollout_id, &error),
                }
            }
            Ok(None) => {
                queue.available();
                POLL_INTERVAL
            }
            Err(error) => {
                queue.unavailable(&error);
                POLL_INTERVAL
            }
        };
        tokio::time::sleep(delay).await;
    }
}

async fn run_rollout(
    client: &PackageInfrastructureClient,
    token: &str,
    rollout_id: &str,
) -> Result<()> {
    let rollout = client.rollout(rollout_id).await?;
    match rollout.state {
        RolloutState::ReadyForReview => {
            tracing::info!(
                operation = "rollout",
                rollout_id = %rollout.rollout_id,
                outcome = "already_ready",
                "package rollout is ready for review"
            );
            return Ok(());
        }
        RolloutState::Validating => {
            complete(client, &rollout.rollout_id, true).await?;
            tracing::info!(
                operation = "rollout",
                rollout_id = %rollout.rollout_id,
                outcome = "recovered",
                "package rollout is ready for review"
            );
            return Ok(());
        }
        RolloutState::Active => {}
        _ => bail!("rollout is not active"),
    }
    let branch = rollout
        .branch
        .as_deref()
        .context("rollout branch is missing")?;
    let root = tempfile::tempdir()?;
    let bundle = root.path().join("rollout.bundle");
    download_bundle(
        &client.rollout_bundle_url(&rollout.rollout_id),
        token,
        &bundle,
    )?;
    let source = root.path().join("source");
    run(
        Command::new("git").arg("clone").arg(&bundle).arg(&source),
        "clone consumer rollout source",
    )?;
    run(
        Command::new("git")
            .arg("-C")
            .arg(&source)
            .args(["switch", branch]),
        "check out consumer rollout branch",
    )?;
    configure_git(&source)?;

    if let Err(error) = update_and_test(
        rollout.ecosystem,
        &rollout.package,
        &rollout.version,
        &source,
    ) {
        return Err(record_failure(client, &rollout.rollout_id, error).await);
    }
    let status = text(
        Command::new("git")
            .arg("-C")
            .arg(&source)
            .args(["status", "--porcelain"]),
        "inspect consumer rollout changes",
    )?;
    if status.trim().is_empty() {
        return Err(record_failure(
            client,
            &rollout.rollout_id,
            anyhow!("package manager produced no consumer dependency change"),
        )
        .await);
    }
    run(
        Command::new("git")
            .arg("-C")
            .arg(&source)
            .args(["add", "--all"]),
        "stage consumer rollout",
    )?;
    run(
        Command::new("git")
            .arg("-C")
            .arg(&source)
            .args(["commit", "--message"])
            .arg(format!(
                "chore(deps): update {} to {}",
                rollout.package, rollout.version
            )),
        "commit consumer rollout",
    )?;
    let submitted = root.path().join("submitted.bundle");
    run(
        Command::new("git")
            .arg("-C")
            .arg(&source)
            .args(["bundle", "create"])
            .arg(&submitted)
            .arg("--all"),
        "bundle tested consumer rollout",
    )?;
    let header = authorization_header(token)?;
    run(
        Command::new("curl")
            .args([
                "--fail",
                "--silent",
                "--show-error",
                "--request",
                "POST",
                "--header",
            ])
            .arg(format!("@{}", header.path().display()))
            .args([
                "--header",
                "Content-Type: application/x-git-bundle",
                "--data-binary",
            ])
            .arg(format!("@{}", submitted.display()))
            .arg(client.rollout_upload_url(&rollout.rollout_id)),
        "submit tested consumer rollout",
    )?;
    let ready = complete(client, &rollout.rollout_id, true).await?;
    tracing::info!(
        operation = "rollout",
        rollout_id = %ready.rollout_id,
        branch = ready.branch.as_deref().unwrap_or("rollout branch"),
        outcome = "ready",
        "package rollout is ready for review"
    );
    Ok(())
}

async fn record_failure(
    client: &PackageInfrastructureClient,
    rollout_id: &str,
    error: Error,
) -> Error {
    match complete(client, rollout_id, false).await {
        Ok(_) => error,
        Err(completion_error) => error.context(format!(
            "failed to persist rejection for rollout {rollout_id}: {completion_error:#}"
        )),
    }
}

async fn complete(
    client: &PackageInfrastructureClient,
    rollout_id: &str,
    passed: bool,
) -> Result<vm_packages::RolloutRecord> {
    client
        .complete_rollout(
            rollout_id,
            &RolloutValidationRequest {
                passed,
                actor: "package-rollout-service".into(),
                idempotency_key: operation_key("rollout", rollout_id),
            },
        )
        .await
}

fn update_and_test(
    ecosystem: PackageEcosystem,
    package: &str,
    version: &str,
    source: &Path,
) -> Result<()> {
    let dependency = format!("{package}@{version}");
    match ecosystem {
        PackageEcosystem::Npm if source.join("pnpm-lock.yaml").is_file() => {
            run(
                Command::new("pnpm")
                    .args(["add", "--save-exact", &dependency])
                    .current_dir(source),
                "update pnpm dependency and lockfile",
            )?;
            run(
                Command::new("pnpm")
                    .args(["run", "--if-present", "test"])
                    .current_dir(source),
                "test pnpm consumer",
            )?;
        }
        PackageEcosystem::Npm if source.join("yarn.lock").is_file() => {
            run(
                Command::new("yarn")
                    .args(["add", "--exact", &dependency])
                    .current_dir(source),
                "update Yarn dependency and lockfile",
            )?;
            run(
                Command::new("npm")
                    .args(["test", "--if-present"])
                    .current_dir(source),
                "test Yarn consumer",
            )?;
        }
        PackageEcosystem::Npm => {
            run(
                Command::new("npm")
                    .args(["install", "--save-exact", &dependency])
                    .current_dir(source),
                "update npm dependency and lockfile",
            )?;
            run(
                Command::new("npm")
                    .args(["test", "--if-present"])
                    .current_dir(source),
                "test npm consumer",
            )?;
        }
        PackageEcosystem::Cargo => {
            run(
                Command::new("cargo")
                    .args(["add", &format!("{package}@={version}")])
                    .current_dir(source),
                "update Cargo dependency and lockfile",
            )?;
            run(
                Command::new("cargo").arg("test").current_dir(source),
                "test Cargo consumer",
            )?;
        }
        PackageEcosystem::Python => {
            run(
                Command::new("uv")
                    .args(["add", &format!("{package}=={version}")])
                    .current_dir(source),
                "update Python dependency and lockfile",
            )?;
            run(
                Command::new("uv")
                    .args(["run", "pytest"])
                    .current_dir(source),
                "test Python consumer",
            )?;
        }
    }
    Ok(())
}

fn configure_git(source: &Path) -> Result<()> {
    run(
        Command::new("git").arg("-C").arg(source).args([
            "config",
            "user.name",
            "VM Package Rollout",
        ]),
        "configure rollout Git identity",
    )?;
    run(
        Command::new("git").arg("-C").arg(source).args([
            "config",
            "user.email",
            "packages@vm.internal",
        ]),
        "configure rollout Git identity",
    )?;
    Ok(())
}
