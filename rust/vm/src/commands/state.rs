use std::path::{Path, PathBuf};

use super::command_context::load_runtime_subject;
use crate::error::{VmError, VmResult};
use vm_config::AppConfig;
use vm_core::{vm_progress, vm_success};

pub(super) async fn save(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    environment: Option<String>,
    snapshot: String,
    description: Option<String>,
    quiesce: bool,
    force: bool,
) -> VmResult<()> {
    let subject = load_runtime_subject(config_path, profile, environment)?;
    let provider = subject.provider.name().to_string();
    let target = subject.target;
    let config = AppConfig {
        global: subject.global_config,
        vm: subject.config,
    };
    vm_progress!("Saving '{target}' as snapshot '{snapshot}'...");
    vm_snapshot::create::handle_create(
        &config,
        &provider,
        &snapshot,
        description.as_deref(),
        quiesce,
        Some(&target),
        None,
        None,
        &[],
        force,
    )
    .await
    .map_err(VmError::from)?;
    vm_success!("Saved snapshot '{snapshot}'");
    Ok(())
}

pub(super) async fn revert(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    environment: Option<String>,
    snapshot: String,
    force: bool,
) -> VmResult<()> {
    let subject = load_runtime_subject(config_path, profile, environment)?;
    let provider = subject.provider.name().to_string();
    let target = subject.target;
    let config = AppConfig {
        global: subject.global_config,
        vm: subject.config,
    };
    vm_progress!("Reverting '{target}' to snapshot '{snapshot}'...");
    vm_snapshot::restore::handle_restore(&config, &provider, &snapshot, Some(&target), force)
        .await
        .map_err(VmError::from)?;
    vm_success!("Reverted '{target}' to snapshot '{snapshot}'");
    Ok(())
}

pub(super) async fn package(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    environment: Option<String>,
    output: Option<PathBuf>,
    compress: u8,
    build: Option<PathBuf>,
) -> VmResult<()> {
    let subject = load_runtime_subject(config_path, profile, environment)?;
    let provider = subject.provider.name().to_string();
    let target = subject.target;
    let config = AppConfig {
        global: subject.global_config,
        vm: subject.config,
    };
    let snapshot = target.as_str();

    if let Some(dockerfile) = build {
        vm_snapshot::create::handle_create(
            &config,
            &provider,
            snapshot,
            Some("Portable base image"),
            false,
            Some(&target),
            Some(&dockerfile),
            Some(Path::new(".")),
            &[],
            true,
        )
        .await?;
    }

    vm_progress!("Packaging snapshot '{snapshot}'...");
    vm_snapshot::export::handle_export(
        &provider,
        snapshot,
        output.as_deref(),
        compress,
        Some(&target),
    )
    .await
    .map_err(VmError::from)?;
    vm_success!("Packaged snapshot '{snapshot}'");
    Ok(())
}

pub(super) fn parse_save(words: &[String]) -> VmResult<(Option<String>, String)> {
    match words {
        [as_word, snapshot] if as_word == "as" => Ok((None, snapshot.clone())),
        [environment, as_word, snapshot] if as_word == "as" => {
            Ok((Some(environment.clone()), snapshot.clone()))
        }
        _ => Err(VmError::validation(
            "Invalid save syntax".to_string(),
            Some("Use: vm save as stable or vm save backend as stable".to_string()),
        )),
    }
}

pub(super) fn parse_revert(words: &[String]) -> VmResult<(Option<String>, String)> {
    match words {
        [snapshot] => Ok((None, snapshot.clone())),
        [environment, snapshot] => Ok((Some(environment.clone()), snapshot.clone())),
        _ => Err(VmError::validation(
            "Invalid revert syntax".to_string(),
            Some("Use: vm revert stable or vm revert backend stable".to_string()),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_save;

    #[test]
    fn parses_save_target_and_snapshot() {
        assert_eq!(
            parse_save(&["backend".into(), "as".into(), "stable".into()]).unwrap(),
            (Some("backend".into()), "stable".into())
        );
    }
}
