use std::path::{Path, PathBuf};

use crate::error::{VmError, VmResult};
use vm_config::AppConfig;

pub(super) async fn save(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    environment: Option<String>,
    snapshot: String,
    description: Option<String>,
    quiesce: bool,
    force: bool,
) -> VmResult<()> {
    let target = StateTarget::load(config_path, profile, environment)?;
    vm_snapshot::create::handle_create(
        &target.config,
        &target.provider,
        &snapshot,
        description.as_deref(),
        quiesce,
        target.environment.as_deref(),
        None,
        None,
        &[],
        force,
    )
    .await
    .map_err(VmError::from)
}

pub(super) async fn revert(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    environment: Option<String>,
    snapshot: String,
    force: bool,
) -> VmResult<()> {
    let target = StateTarget::load(config_path, profile, environment)?;
    vm_snapshot::restore::handle_restore(
        &target.config,
        &target.provider,
        &snapshot,
        target.environment.as_deref(),
        force,
    )
    .await
    .map_err(VmError::from)
}

pub(super) async fn package(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    environment: Option<String>,
    output: Option<PathBuf>,
    compress: u8,
    build: Option<PathBuf>,
) -> VmResult<()> {
    let target = StateTarget::load(config_path, profile, environment)?;
    let snapshot = target.environment.as_deref().unwrap_or("environment");

    if let Some(dockerfile) = build {
        vm_snapshot::create::handle_create(
            &target.config,
            &target.provider,
            snapshot,
            Some("Portable base image"),
            false,
            target.environment.as_deref(),
            Some(&dockerfile),
            Some(Path::new(".")),
            &[],
            true,
        )
        .await?;
    }

    vm_snapshot::export::handle_export(
        &target.provider,
        snapshot,
        output.as_deref(),
        compress,
        target.environment.as_deref(),
    )
    .await
    .map_err(VmError::from)
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

struct StateTarget {
    config: AppConfig,
    provider: String,
    environment: Option<String>,
}

impl StateTarget {
    fn load(
        config_path: Option<PathBuf>,
        profile: Option<String>,
        environment: Option<String>,
    ) -> VmResult<Self> {
        let config = AppConfig::load(config_path, profile, None)?;
        let provider = config
            .vm
            .provider
            .clone()
            .unwrap_or_else(|| "docker".to_string());
        let environment = environment.or_else(|| {
            config
                .vm
                .project
                .as_ref()
                .and_then(|project| project.name.clone())
        });
        Ok(Self {
            config,
            provider,
            environment,
        })
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
