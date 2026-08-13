use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use clap::Parser;
use vm_package_jobs::runtime::{
    command_text as text, operation_key, required_secret as secret, run_command as run,
};
use vm_packages::{
    PackageEcosystem, PackageInfrastructureClient, RegistryEndpoints, RolloutState,
    RolloutValidationRequest,
};

#[derive(Parser)]
#[command(
    name = "pkg-rollout",
    version,
    about = "Ephemeral isolated consumer-upgrade runner"
)]
struct Cli {
    #[arg(long, required_unless_present = "watch")]
    rollout: Option<String>,
    #[arg(long)]
    watch: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let gateway =
        std::env::var("PKG_ROLLOUT_GATEWAY").unwrap_or_else(|_| "http://gateway:8080".into());
    let token = secret("PKG_ROLLOUT_TOKEN_FILE")?;
    let client = PackageInfrastructureClient::new(RegistryEndpoints::new(gateway)?)
        .with_rollout_token(token.clone());
    if cli.watch {
        loop {
            match client.next_rollout().await {
                Ok(Some(rollout)) => {
                    if let Err(error) = run_rollout(&client, &token, &rollout.rollout_id).await {
                        eprintln!("rollout {} failed: {error:#}", rollout.rollout_id);
                    }
                }
                Ok(None) => {}
                Err(error) => eprintln!("rollout queue unavailable: {error:#}"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    }
    run_rollout(
        &client,
        &token,
        cli.rollout.as_deref().context("--rollout is required")?,
    )
    .await
}

async fn run_rollout(
    client: &PackageInfrastructureClient,
    token: &str,
    rollout_id: &str,
) -> Result<()> {
    let rollout = client.rollout(rollout_id).await?;
    match rollout.state {
        RolloutState::ReadyForReview => {
            println!("{} is already ready for review", rollout.rollout_id);
            return Ok(());
        }
        RolloutState::Validating => {
            complete(&client, &rollout.rollout_id, true).await?;
            println!("{} recovered and is ready for review", rollout.rollout_id);
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
    let header = root.path().join("authorization-header");
    fs::write(&header, format!("Authorization: Bearer {token}\n"))?;
    run(
        Command::new("curl")
            .args([
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--header",
            ])
            .arg(format!("@{}", header.display()))
            .arg("--output")
            .arg(&bundle)
            .arg(client.rollout_bundle_url(&rollout.rollout_id)),
        "download consumer rollout source",
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
        let _ = complete(&client, &rollout.rollout_id, false).await;
        return Err(error);
    }
    let status = text(
        Command::new("git")
            .arg("-C")
            .arg(&source)
            .args(["status", "--porcelain"]),
        "inspect consumer rollout changes",
    )?;
    if status.trim().is_empty() {
        let _ = complete(&client, &rollout.rollout_id, false).await;
        bail!("package manager produced no consumer dependency change");
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
            .arg(format!("@{}", header.display()))
            .args([
                "--header",
                "Content-Type: application/x-git-bundle",
                "--data-binary",
            ])
            .arg(format!("@{}", submitted.display()))
            .arg(client.rollout_upload_url(&rollout.rollout_id)),
        "submit tested consumer rollout",
    )?;
    let ready = complete(&client, &rollout.rollout_id, true).await?;
    println!(
        "{} is ready for review on {}",
        ready.rollout_id,
        ready.branch.as_deref().unwrap_or("rollout branch")
    );
    Ok(())
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
