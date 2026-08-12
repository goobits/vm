use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::process::Command;
use vm_config::config::VmConfig;
use vm_core::error::{Result, VmError};

const STATE_DIRECTORY: &str = "tart";
const STATE_FILE: &str = "instances.json";
const LOCK_FILE: &str = "instances.lock";

#[derive(Debug, Default, Deserialize, Serialize)]
struct StorageState {
    #[serde(default)]
    instances: BTreeMap<String, PathBuf>,
}

pub(super) fn configured_home(config: Option<&VmConfig>) -> Option<PathBuf> {
    config
        .and_then(|config| config.tart.as_ref())
        .and_then(|tart| tart.storage_path.as_deref())
        .filter(|path| !path.trim().is_empty())
        .map(expand_home)
        .or_else(|| {
            std::env::var_os("TART_HOME")
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
        })
}

pub(super) fn resolve_project_home(config: &VmConfig, project: &str) -> Result<Option<PathBuf>> {
    if let Some(home) = configured_home(Some(config)) {
        return Ok(Some(home));
    }

    let state = read_state()?;
    if let Some(home) = state
        .instances
        .iter()
        .find(|(instance, _)| belongs_to_project(instance, project))
        .map(|(_, home)| home.clone())
    {
        return Ok(Some(home));
    }

    recover_running_project_home(project)
}

pub(super) fn remember_instance(instance: &str, home: &Path) -> Result<()> {
    validate_instance(instance)?;
    let state_dir = state_dir()?;
    fs::create_dir_all(&state_dir)?;
    set_mode(&state_dir, 0o700)?;
    let lock = lock(&state_dir)?;
    let path = state_dir.join(STATE_FILE);
    let mut state = read_state_at(&path)?;
    state
        .instances
        .insert(instance.to_string(), home.to_path_buf());
    let mut content = serde_json::to_vec_pretty(&state)?;
    content.push(b'\n');
    vm_core::file_system::atomic_write(&path, &content)?;
    set_mode(&path, 0o600)?;
    FileExt::unlock(&lock)?;
    Ok(())
}

#[cfg(feature = "tart")]
pub(super) fn forget_instance(instance: &str) -> Result<()> {
    validate_instance(instance)?;
    let state_dir = state_dir()?;
    if !state_dir.exists() {
        return Ok(());
    }
    let lock = lock(&state_dir)?;
    let path = state_dir.join(STATE_FILE);
    let mut state = read_state_at(&path)?;
    if state.instances.remove(instance).is_some() {
        let mut content = serde_json::to_vec_pretty(&state)?;
        content.push(b'\n');
        vm_core::file_system::atomic_write(&path, &content)?;
        set_mode(&path, 0o600)?;
    }
    FileExt::unlock(&lock)?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn recover_running_project_home(_project: &str) -> Result<Option<PathBuf>> {
    Ok(None)
}

#[cfg(target_os = "macos")]
fn recover_running_project_home(project: &str) -> Result<Option<PathBuf>> {
    let output = match Command::new("ps").args(["-axo", "pid=,command="]).output() {
        Ok(output) if output.status.success() => output,
        _ => return Ok(None),
    };
    let processes = parse_running_tart_processes(&String::from_utf8_lossy(&output.stdout));
    for (pid, instance) in processes
        .into_iter()
        .filter(|(_, instance)| belongs_to_project(instance, project))
    {
        let output = match Command::new("lsof")
            .args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])
            .output()
        {
            Ok(output) if output.status.success() => output,
            _ => continue,
        };
        if let Some(home) =
            parse_tart_home_from_lsof(&String::from_utf8_lossy(&output.stdout), &instance)
        {
            remember_instance(&instance, &home)?;
            return Ok(Some(home));
        }
    }
    Ok(None)
}

fn read_state() -> Result<StorageState> {
    read_state_at(&state_dir()?.join(STATE_FILE))
}

fn read_state_at(path: &Path) -> Result<StorageState> {
    match fs::read(path) {
        Ok(content) => serde_json::from_slice(&content).map_err(Into::into),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(StorageState::default()),
        Err(error) => Err(error.into()),
    }
}

fn state_dir() -> Result<PathBuf> {
    Ok(vm_core::user_paths::vm_state_dir()?.join(STATE_DIRECTORY))
}

fn lock(state_dir: &Path) -> Result<File> {
    let path = state_dir.join(LOCK_FILE);
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)?;
    set_mode(&path, 0o600)?;
    file.lock_exclusive()?;
    Ok(file)
}

fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        return vm_core::user_paths::home_dir().unwrap_or_else(|_| PathBuf::from(path));
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = vm_core::user_paths::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

fn belongs_to_project(instance: &str, project: &str) -> bool {
    instance == project
        || instance
            .strip_prefix(project)
            .is_some_and(|suffix| suffix.starts_with('-'))
}

fn validate_instance(instance: &str) -> Result<()> {
    if instance.is_empty()
        || !instance.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
        || instance == "."
        || instance == ".."
    {
        return Err(VmError::Validation(format!(
            "Invalid Tart instance name '{instance}'"
        )));
    }
    Ok(())
}

#[cfg(any(target_os = "macos", test))]
fn parse_running_tart_processes(output: &str) -> Vec<(u32, String)> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let split = line.find(char::is_whitespace)?;
            let pid = line[..split].parse().ok()?;
            let command = line[split..].trim();
            let words: Vec<&str> = command.split_whitespace().collect();
            let _run = words.windows(2).position(|pair| {
                Path::new(pair[0])
                    .file_name()
                    .is_some_and(|name| name == "tart")
                    && pair[1] == "run"
            })?;
            let instance = words.last()?.trim_matches(['\'', '"']);
            validate_instance(instance).ok()?;
            Some((pid, instance.to_string()))
        })
        .collect()
}

#[cfg(any(target_os = "macos", test))]
fn parse_tart_home_from_lsof(output: &str, instance: &str) -> Option<PathBuf> {
    output.lines().find_map(|line| {
        let path = Path::new(line.strip_prefix('n')?);
        if path.file_name()? != instance || path.parent()?.file_name()? != "vms" {
            return None;
        }
        path.parent()?.parent().map(Path::to_path_buf)
    })
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{belongs_to_project, parse_running_tart_processes, parse_tart_home_from_lsof};
    use std::path::PathBuf;

    #[test]
    fn parses_only_tart_run_processes() {
        let output = "70525 /opt/homebrew/bin/tart run --no-graphics --dir /Users/me/project:tag=workspace vm-mac\n70526 tart list\n";
        assert_eq!(
            parse_running_tart_processes(output),
            vec![(70525, "vm-mac".to_string())]
        );
    }

    #[test]
    fn derives_home_only_from_exact_instance_directory() {
        let output = "p70525\nfcwd\nn/Volumes/External/Tart/vms/vm-mac\n";
        assert_eq!(
            parse_tart_home_from_lsof(output, "vm-mac"),
            Some(PathBuf::from("/Volumes/External/Tart"))
        );
        assert_eq!(parse_tart_home_from_lsof(output, "other"), None);
    }

    #[test]
    fn project_matching_does_not_use_ambiguous_prefixes() {
        assert!(belongs_to_project("vm", "vm"));
        assert!(belongs_to_project("vm-mac", "vm"));
        assert!(!belongs_to_project("vmmac", "vm"));
    }
}
