use std::path::Path;

use anyhow::{bail, Result};

use super::build::run_isolated;

pub(super) fn restore_locked_node_dependencies(
    source: &Path,
    release_root: &Path,
    command: Option<&[String]>,
) -> Result<()> {
    let Some(command) = command else {
        return Ok(());
    };
    run_isolated(
        command,
        source,
        release_root,
        "restore locked Node dependencies",
    )
}

pub(super) fn locked_node_install(source: &Path) -> Result<Option<Vec<String>>> {
    let pnpm = source.join("pnpm-lock.yaml").is_file();
    let yarn = source.join("yarn.lock").is_file();
    let npm =
        source.join("package-lock.json").is_file() || source.join("npm-shrinkwrap.json").is_file();
    let lockfile_count = [pnpm, yarn, npm]
        .into_iter()
        .filter(|present| *present)
        .count();

    if lockfile_count == 0 {
        return Ok(None);
    }
    if !source.join("package.json").is_file() {
        bail!("Node lockfile requires package.json in the binary tool source");
    }
    if lockfile_count > 1 {
        bail!("binary tool source contains multiple Node package-manager lockfiles");
    }

    let command = if pnpm {
        ["pnpm", "install", "--frozen-lockfile", "--ignore-scripts"].as_slice()
    } else if yarn {
        [
            "yarn",
            "install",
            "--frozen-lockfile",
            "--ignore-scripts",
            "--non-interactive",
        ]
        .as_slice()
    } else {
        ["npm", "ci", "--ignore-scripts"].as_slice()
    };
    Ok(Some(
        command.iter().map(|argument| (*argument).into()).collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_with(files: &[&str]) -> tempfile::TempDir {
        let source = tempfile::tempdir().unwrap();
        for file in files {
            std::fs::write(source.path().join(file), "fixture").unwrap();
        }
        source
    }

    #[test]
    fn pnpm_lockfile_selects_frozen_install() {
        let source = source_with(&["package.json", "pnpm-lock.yaml"]);

        assert_eq!(
            locked_node_install(source.path()).unwrap().unwrap(),
            ["pnpm", "install", "--frozen-lockfile", "--ignore-scripts"]
        );
    }

    #[test]
    fn npm_lockfile_selects_clean_install() {
        let source = source_with(&["package.json", "package-lock.json"]);

        assert_eq!(
            locked_node_install(source.path()).unwrap().unwrap(),
            ["npm", "ci", "--ignore-scripts"]
        );
    }

    #[test]
    fn source_without_node_lockfile_is_unchanged() {
        let source = source_with(&["package.json"]);

        assert_eq!(locked_node_install(source.path()).unwrap(), None);
    }

    #[test]
    fn ambiguous_node_lockfiles_fail_closed() {
        let source = source_with(&["package.json", "pnpm-lock.yaml", "yarn.lock"]);

        assert_eq!(
            locked_node_install(source.path()).unwrap_err().to_string(),
            "binary tool source contains multiple Node package-manager lockfiles"
        );
    }
}
