use std::process::{Command, Stdio};

use crate::error::{VmError, VmResult};

#[cfg(any(test, target_os = "macos"))]
const WORKER_COMMAND_PATH: &str =
    "/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin";

#[cfg(target_os = "linux")]
pub(super) fn install(executable: &std::path::Path) -> VmResult<bool> {
    let directory = vm_core::user_paths::home_dir()?.join(".config/systemd/user");
    std::fs::create_dir_all(&directory).map_err(VmError::from)?;
    let path = directory.join("vm-tool-activation.service");
    let executable = executable.to_string_lossy().replace('%', "%%");
    write_if_changed(&path, systemd_service(&executable).as_bytes())?;
    let reloaded = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if !reloaded.is_ok_and(|status| status.success()) {
        return Ok(false);
    }
    let enabled = Command::new("systemctl")
        .args(["--user", "enable", "--now", "vm-tool-activation.service"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !enabled {
        return Ok(false);
    }
    Ok(Command::new("systemctl")
        .args(["--user", "restart", "vm-tool-activation.service"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success()))
}

#[cfg(target_os = "macos")]
pub(super) fn install(executable: &std::path::Path) -> VmResult<bool> {
    let directory = vm_core::user_paths::home_dir()?.join("Library/LaunchAgents");
    std::fs::create_dir_all(&directory).map_err(VmError::from)?;
    let path = directory.join("com.goobits.vm-tool-activation.plist");
    let executable = xml_escape(&executable.to_string_lossy());
    let changed = write_if_changed(&path, launchd_service(&executable).as_bytes())?;
    let domain = launchd_domain()?;
    let label = format!("{domain}/com.goobits.vm-tool-activation");
    if changed {
        let _ = Command::new("launchctl")
            .args(["bootout", &label])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    if !Command::new("launchctl")
        .args(["print", &label])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
    {
        let status = Command::new("launchctl")
            .arg("bootstrap")
            .arg(&domain)
            .arg(&path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if !status.is_ok_and(|status| status.success()) {
            return Ok(false);
        }
    }
    Ok(Command::new("launchctl")
        .args(["kickstart", "-k", &label])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success()))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(super) fn install(_executable: &std::path::Path) -> VmResult<bool> {
    Ok(false)
}

#[cfg(target_os = "linux")]
pub(super) fn remove() -> VmResult<()> {
    let path =
        vm_core::user_paths::home_dir()?.join(".config/systemd/user/vm-tool-activation.service");
    let _ = Command::new("systemctl")
        .args(["--user", "disable", "--now", "vm-tool-activation.service"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    remove_if_present(&path)?;
    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    Ok(())
}

#[cfg(target_os = "macos")]
pub(super) fn remove() -> VmResult<()> {
    let path = vm_core::user_paths::home_dir()?
        .join("Library/LaunchAgents/com.goobits.vm-tool-activation.plist");
    if let Ok(domain) = launchd_domain() {
        let _ = Command::new("launchctl")
            .args([
                "bootout",
                &format!("{domain}/com.goobits.vm-tool-activation"),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    remove_if_present(&path)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(super) fn remove() -> VmResult<()> {
    Ok(())
}

fn write_if_changed(path: &std::path::Path, content: &[u8]) -> VmResult<bool> {
    if std::fs::read(path).is_ok_and(|current| current == content) {
        return Ok(false);
    }
    vm_core::file_system::atomic_write(path, content).map_err(VmError::from)?;
    Ok(true)
}

fn remove_if_present(path: &std::path::Path) -> VmResult<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(VmError::from(error)),
    }
}

#[cfg(target_os = "macos")]
fn launchd_domain() -> VmResult<String> {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .map_err(VmError::from)?;
    let uid = std::str::from_utf8(&output.stdout)
        .map_err(|_| {
            VmError::validation("Could not resolve the launchd user domain", None::<String>)
        })?
        .trim();
    if !output.status.success()
        || uid.is_empty()
        || !uid.chars().all(|character| character.is_ascii_digit())
    {
        return Err(VmError::validation(
            "Could not resolve the launchd user domain",
            None::<String>,
        ));
    }
    Ok(format!("gui/{uid}"))
}

#[cfg(any(test, target_os = "macos"))]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(any(test, target_os = "linux"))]
fn systemd_service(executable: &str) -> String {
    format!(
        "[Unit]\nDescription=VM managed-tool activation\n\n[Service]\nType=simple\nExecStart={executable:?} tools activation-worker\nRestart=always\nRestartSec=2\n\n[Install]\nWantedBy=default.target\n"
    )
}

#[cfg(any(test, target_os = "macos"))]
fn launchd_service(executable: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>com.goobits.vm-tool-activation</string>\n<key>ProgramArguments</key><array><string>{executable}</string><string>tools</string><string>activation-worker</string></array>\n<key>EnvironmentVariables</key><dict><key>PATH</key><string>{WORKER_COMMAND_PATH}</string></dict>\n<key>RunAtLoad</key><true/>\n<key>KeepAlive</key><true/>\n<key>ProcessType</key><string>Background</string>\n</dict></plist>\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn systemd_worker_runs_the_activation_command() {
        let service = systemd_service("/home/example/.local/bin/vm");
        assert!(
            service.contains("ExecStart=\"/home/example/.local/bin/vm\" tools activation-worker")
        );
        assert!(service.contains("Restart=always"));
    }

    #[test]
    fn launchd_worker_can_resolve_host_providers() {
        let executable = xml_escape("/Users/example/A&B/vm");
        let service = launchd_service(&executable);
        assert!(service.contains("/Users/example/A&amp;B/vm"));
        assert!(service.contains("/opt/homebrew/bin"));
        assert!(service.contains("/usr/local/bin"));
    }

    #[cfg(unix)]
    #[test]
    fn service_write_does_not_follow_the_old_predictable_temp_path() {
        let directory = tempfile::tempdir().unwrap();
        let service = directory.path().join("service.conf");
        let old_temporary = service.with_extension(format!("tmp-{}", std::process::id()));
        let victim = directory.path().join("victim");
        std::fs::write(&victim, "owner-data").unwrap();
        std::os::unix::fs::symlink(&victim, old_temporary).unwrap();

        assert!(write_if_changed(&service, b"service-data").unwrap());
        assert_eq!(std::fs::read_to_string(service).unwrap(), "service-data");
        assert_eq!(std::fs::read_to_string(victim).unwrap(), "owner-data");
    }
}
