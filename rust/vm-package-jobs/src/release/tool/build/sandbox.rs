use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::runtime::run_command;

pub(super) fn prepare_unprivileged_build(
    root: &Path,
    source: &Path,
    build_sources: &[PathBuf],
) -> Result<()> {
    let Some(uid) = std::env::var_os("PKG_BUILD_UID") else {
        return Ok(());
    };
    let gid = std::env::var_os("PKG_BUILD_GID").context("PKG_BUILD_GID is required")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = std::fs::metadata(root)?.permissions();
        permissions.set_mode(0o711);
        std::fs::set_permissions(root, permissions)?;
    }
    let sandbox = root.join("untrusted");
    for directory in sandbox_directories(&sandbox) {
        std::fs::create_dir_all(directory)?;
    }
    let mut chown = Command::new("chown");
    chown.arg("-R").arg(format!(
        "{}:{}",
        uid.to_string_lossy(),
        gid.to_string_lossy()
    ));
    chown.arg(source);
    for build_source in build_sources {
        chown.arg(build_source);
    }
    chown.arg(&sandbox);
    run_command(&mut chown, "prepare unprivileged binary build workspace")?;
    Ok(())
}

pub(in crate::release::tool) fn run_isolated(
    arguments: &[String],
    directory: &Path,
    release_root: &Path,
    operation: &str,
) -> Result<()> {
    let (program, arguments) = arguments
        .split_first()
        .context("isolated command cannot be empty")?;
    let program_name = program.clone();
    let sandbox = release_root.join("untrusted");
    let sandbox = if sandbox.is_dir() {
        sandbox.as_path()
    } else {
        release_root
    };
    let package_gateway = package_gateway();
    let cargo_home = sandbox.join("cargo-home");
    std::fs::create_dir_all(&cargo_home).context("create isolated Cargo home")?;
    let cargo_config = cargo_home.join("config.toml");
    if !cargo_config.is_file() {
        std::fs::write(&cargo_config, cargo_source_config(&package_gateway)?)
            .context("write isolated Cargo source configuration")?;
    }

    let mut command = Command::new("timeout");
    command
        .args(["--signal=TERM", "--kill-after=10s", "30m"])
        .arg(program)
        .args(arguments)
        .current_dir(directory)
        .env_clear()
        .env("HOME", sandbox)
        .env("TMPDIR", sandbox)
        .env("XDG_CACHE_HOME", sandbox.join("xdg-cache"))
        .env("CARGO_HOME", cargo_home)
        .env("CARGO_TARGET_DIR", sandbox.join("cargo-target"))
        .env("npm_config_cache", sandbox.join("npm-cache"))
        .env("PIP_CACHE_DIR", sandbox.join("pip-cache"))
        .env("NPM_CONFIG_REGISTRY", format!("{package_gateway}/npm/"))
        .env("PIP_INDEX_URL", format!("{package_gateway}/pypi/simple/"))
        .env("UV_INDEX_URL", format!("{package_gateway}/pypi/simple/"))
        .env(
            "CARGO_REGISTRIES_VM_INDEX",
            format!("sparse+{package_gateway}/cargo/index/"),
        )
        .env("CARGO_SOURCE_CRATES_IO_REPLACE_WITH", "vm")
        .env(
            "CARGO_SOURCE_VM_REGISTRY",
            format!("sparse+{package_gateway}/cargo/index/"),
        );
    for variable in ["PATH", "RUSTUP_HOME"] {
        if let Some(value) = std::env::var_os(variable) {
            command.env(variable, value);
        }
    }
    #[cfg(unix)]
    if let Some(uid) = std::env::var_os("PKG_BUILD_UID") {
        use std::os::unix::process::CommandExt;

        let uid = uid
            .to_string_lossy()
            .parse::<u32>()
            .context("PKG_BUILD_UID must be a numeric user ID")?;
        let gid = std::env::var("PKG_BUILD_GID")
            .context("PKG_BUILD_GID is required")?
            .parse::<u32>()
            .context("PKG_BUILD_GID must be a numeric group ID")?;
        command.uid(uid).gid(gid);
    }
    run_command(&mut command, operation).with_context(|| {
        format!(
            "run isolated program `{program_name}` via `timeout` from {}",
            directory.display()
        )
    })?;
    Ok(())
}

pub(in crate::release::tool) fn prepare_isolated_package_configuration(
    release_root: &Path,
) -> Result<()> {
    let cargo_home = release_root.join("untrusted/cargo-home");
    std::fs::create_dir_all(&cargo_home).context("create isolated Cargo home")?;
    std::fs::write(
        cargo_home.join("config.toml"),
        cargo_source_config(&package_gateway())?,
    )
    .context("write isolated Cargo source configuration")
}

pub(in crate::release::tool) fn cargo_source_config(package_gateway: &str) -> Result<String> {
    let gateway = url::Url::parse(package_gateway).context("parse package build gateway")?;
    if !matches!(gateway.scheme(), "http" | "https") {
        bail!("package build gateway must use HTTP(S)");
    }
    let registry = format!(
        "sparse+{}/cargo/index/",
        gateway.as_str().trim_end_matches('/')
    );
    Ok(format!(
        "[source.crates-io]\nreplace-with = \"vm\"\n\n[source.vm]\nregistry = \"{registry}\"\n"
    ))
}

pub(in crate::release::tool) fn native_target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("linux-amd64"),
        ("linux", "aarch64") => Some("linux-arm64"),
        _ => None,
    }
}

fn package_gateway() -> String {
    std::env::var("PKG_BUILD_PACKAGE_GATEWAY")
        .unwrap_or_else(|_| "http://build-edge:3080".into())
        .trim_end_matches('/')
        .to_string()
}

fn sandbox_directories(sandbox: &Path) -> [PathBuf; 5] {
    [
        sandbox.join("cargo-home"),
        sandbox.join("cargo-target"),
        sandbox.join("npm-cache"),
        sandbox.join("pip-cache"),
        sandbox.join("xdg-cache"),
    ]
}
