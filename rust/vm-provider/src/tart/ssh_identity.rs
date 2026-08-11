use fs2::FileExt;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::IsTerminal;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

use crate::{shell_session, VmError};
use vm_core::error::Result;
use vm_core::{vm_println, vm_warning};

const KEY_FILE: &str = "tart_ed25519";
const KEY_COMMENT: &str = "vm-tart";

#[derive(Clone, Debug)]
pub(super) struct TartSshIdentity {
    private_key: PathBuf,
    public_key: String,
}

impl TartSshIdentity {
    pub(super) fn ensure() -> Result<Self> {
        let directory = vm_core::user_paths::vm_state_dir()?.join("ssh");
        fs::create_dir_all(&directory)?;
        set_mode(&directory, 0o700)?;
        let lock = key_lock(&directory)?;
        let private_key = directory.join(KEY_FILE);
        let public_key = private_key.with_extension("pub");
        reject_symlink(&private_key)?;
        reject_symlink(&public_key)?;

        if !private_key.exists() {
            let status = Command::new("ssh-keygen")
                .args(["-q", "-t", "ed25519", "-N", "", "-C", KEY_COMMENT, "-f"])
                .arg(&private_key)
                .status()
                .map_err(|error| {
                    VmError::Provider(format!("Failed to generate Tart SSH identity: {error}"))
                })?;
            if !status.success() {
                return Err(VmError::Provider(format!(
                    "ssh-keygen failed while creating {}",
                    private_key.display()
                )));
            }
        }

        set_mode(&private_key, 0o600)?;
        let key = read_or_derive_public_key(&private_key, &public_key)?;
        set_mode(&public_key, 0o644)?;
        FileExt::unlock(&lock)?;
        Ok(Self {
            private_key,
            public_key: validate_public_key(&key)?,
        })
    }

    pub(super) fn authorized_key_script(&self) -> String {
        let key = shell_session::quote_posix_argument(&self.public_key);
        format!(
            r#"set -e
umask 077
mkdir -p "$HOME/.ssh"
touch "$HOME/.ssh/authorized_keys"
if ! grep -Fqx {key} "$HOME/.ssh/authorized_keys"; then
  printf '%s\n' {key} >> "$HOME/.ssh/authorized_keys"
fi
chmod 700 "$HOME/.ssh"
chmod 600 "$HOME/.ssh/authorized_keys""#
        )
    }

    pub(super) fn ensure_authorized(&self, user: &str, ip: IpAddr) -> Result<()> {
        if self.probe(user, ip) {
            return Ok(());
        }
        if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
            return Err(VmError::Provider(format!(
                "Tart SSH key is not installed for {user}@{ip}; run `vm ssh` from an interactive terminal once"
            )));
        }

        vm_warning!(
            "Tart guest agent unavailable; one guest password is required to install the managed SSH key"
        );
        let mut command = self.base_command(false, false);
        command.args([
            "-o",
            "PubkeyAuthentication=no",
            "-o",
            "PreferredAuthentications=keyboard-interactive,password",
            "-o",
            "NumberOfPasswordPrompts=1",
        ]);
        command
            .arg(format!("{user}@{ip}"))
            .arg(self.authorized_key_script());
        let status = command.status().map_err(|error| {
            VmError::Provider(format!("Failed to bootstrap Tart SSH identity: {error}"))
        })?;
        if !status.success() || !self.probe(user, ip) {
            return Err(VmError::Provider(
                "Tart SSH identity bootstrap did not complete; verify the guest password and retry"
                    .to_string(),
            ));
        }
        vm_println!("Managed Tart SSH key installed");
        Ok(())
    }

    pub(super) fn interactive(
        &self,
        user: &str,
        ip: IpAddr,
        remote_command: &str,
    ) -> Result<ExitStatus> {
        self.command(user, ip, true, true)
            .arg(remote_command)
            .status()
            .map_err(|error| VmError::Provider(format!("SSH failed: {error}")))
    }

    fn probe(&self, user: &str, ip: IpAddr) -> bool {
        self.command(user, ip, false, true)
            .arg("true")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    fn command(&self, user: &str, ip: IpAddr, tty: bool, batch: bool) -> Command {
        let mut command = self.base_command(tty, batch);
        command.arg(format!("{user}@{ip}"));
        command
    }

    fn base_command(&self, tty: bool, batch: bool) -> Command {
        let mut command = Command::new("ssh");
        if tty {
            command.arg("-t");
        }
        command.arg("-i").arg(&self.private_key).args([
            "-o",
            "IdentitiesOnly=yes",
            "-o",
            "ConnectTimeout=5",
            "-o",
            "ConnectionAttempts=1",
            "-o",
            "ServerAliveInterval=30",
            "-o",
            "ServerAliveCountMax=3",
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=/dev/null",
            "-o",
            "LogLevel=ERROR",
            "-o",
            if batch {
                "BatchMode=yes"
            } else {
                "BatchMode=no"
            },
        ]);
        command
    }
}

fn key_lock(directory: &Path) -> Result<File> {
    let path = directory.join("tart-key.lock");
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

fn read_or_derive_public_key(private_key: &Path, public_key: &Path) -> Result<String> {
    if public_key.exists() {
        return fs::read_to_string(public_key).map_err(Into::into);
    }
    let output = Command::new("ssh-keygen")
        .args([OsStr::new("-y"), OsStr::new("-f")])
        .arg(private_key)
        .output()
        .map_err(|error| {
            VmError::Provider(format!("Failed to derive Tart SSH public key: {error}"))
        })?;
    if !output.status.success() {
        return Err(VmError::Provider(
            "Failed to derive Tart SSH public key".to_string(),
        ));
    }
    vm_core::file_system::atomic_write(public_key, &output.stdout)?;
    String::from_utf8(output.stdout)
        .map_err(|error| VmError::Provider(format!("Invalid SSH public key encoding: {error}")))
}

fn validate_public_key(value: &str) -> Result<String> {
    let key = value.trim();
    if key.lines().count() != 1 || !key.starts_with("ssh-ed25519 ") {
        return Err(VmError::Provider(
            "Managed Tart SSH public key is invalid".to_string(),
        ));
    }
    Ok(key.to_string())
}

fn reject_symlink(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(VmError::Provider(format!(
            "Refusing symlinked Tart SSH identity path: {}",
            path.display()
        ))),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
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
    use super::{validate_public_key, TartSshIdentity};
    use std::path::PathBuf;

    fn identity() -> TartSshIdentity {
        TartSshIdentity {
            private_key: PathBuf::from("/tmp/tart key"),
            public_key: "ssh-ed25519 AAAATEST vm-tart".to_string(),
        }
    }

    #[test]
    fn ssh_command_uses_only_the_managed_identity() {
        let command = identity().command("admin", "192.168.64.37".parse().unwrap(), true, true);
        let args = command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert!(args.iter().any(|arg| arg == "IdentitiesOnly=yes"));
        assert!(args.iter().any(|arg| arg == "BatchMode=yes"));
        assert!(args.iter().any(|arg| arg == "UserKnownHostsFile=/dev/null"));
        assert!(args.iter().any(|arg| arg == "/tmp/tart key"));
        assert_eq!(args.last().map(String::as_str), Some("admin@192.168.64.37"));
    }

    #[test]
    fn authorized_key_install_is_idempotent_and_private() {
        let script = identity().authorized_key_script();
        assert!(script.contains("grep -Fqx"));
        assert!(script.contains("chmod 600"));
        assert!(script.contains("ssh-ed25519 AAAATEST"));
    }

    #[test]
    fn rejects_multiline_or_non_ed25519_public_keys() {
        assert!(validate_public_key("ssh-rsa AAAATEST").is_err());
        assert!(validate_public_key("ssh-ed25519 AAAA\nssh-ed25519 BBBB").is_err());
    }
}
