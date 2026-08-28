use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use vm_core::vm_warning;

use crate::{VmError, VmResult};

use super::{TartCommand, VIBE_AI_TOOLS_INSTALLER};

#[derive(Debug, Deserialize, Serialize)]
struct BaseReceipt {
    name: String,
    guest_os: String,
    controller_version: String,
    source: String,
    tart_home: Option<PathBuf>,
}

pub(super) fn pull(
    command: &TartCommand,
    image: &str,
    base_name: &str,
    guest_os: &str,
) -> VmResult<bool> {
    let staging = temporary_name(base_name, "staging");
    let output = command
        .command()
        .args(["clone", image, &staging])
        .output()
        .map_err(|error| VmError::general(error, format!("Failed to pull Tart base '{image}'")))?;
    if !output.status.success() {
        let details = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !details.is_empty() {
            vm_warning!("Could not pull prebuilt Tart base: {details}");
        }
        delete(command, &staging);
        return Ok(false);
    }
    if let Err(error) = install(command, &staging, base_name) {
        delete(command, &staging);
        return Err(error);
    }
    write_receipt(command, base_name, guest_os, image)?;
    Ok(true)
}

pub(super) fn build(
    tart: &TartCommand,
    guest_os: &str,
    base_name: &str,
    builder: &str,
) -> VmResult<()> {
    let staging = temporary_name(base_name, "staging");
    let mut command = Command::new("bash");
    tart.configure(&mut command);
    command.env("VIBE_AI_TOOLS_INSTALLER", VIBE_AI_TOOLS_INSTALLER);
    command.args([
        "-c",
        builder,
        "vm-tart-base-builder",
        "--guest-os",
        guest_os,
        "--name",
        &staging,
    ]);
    if let Err(error) = run_host(command, "build Tart vibe base") {
        delete(tart, &staging);
        return Err(error);
    }
    if let Err(error) = install(tart, &staging, base_name) {
        delete(tart, &staging);
        return Err(error);
    }
    write_receipt(tart, base_name, guest_os, "local-build")
}

pub(super) fn receipt_matches(
    tart: &TartCommand,
    base_name: &str,
    guest_os: &str,
) -> VmResult<bool> {
    let path = receipt_path(base_name, tart.home())?;
    let receipt: BaseReceipt = match fs::read(&path) {
        Ok(content) => serde_json::from_slice(&content).map_err(|error| {
            VmError::general(error, format!("Failed to read {}", path.display()))
        })?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(VmError::from(error)),
    };
    Ok(receipt.name == base_name
        && receipt.guest_os == guest_os
        && receipt.controller_version == env!("CARGO_PKG_VERSION")
        && receipt.tart_home.as_deref() == tart.home())
}

fn install(tart: &TartCommand, staging: &str, final_name: &str) -> VmResult<()> {
    let backup = temporary_name(final_name, "backup");
    let had_previous = exists(tart, final_name)?;
    if had_previous {
        run_tart(
            tart,
            &["rename", final_name, &backup],
            "stage existing Tart base",
        )?;
    }

    if let Err(error) = run_tart(tart, &["rename", staging, final_name], "activate Tart base") {
        if had_previous {
            let _ = run_tart(
                tart,
                &["rename", &backup, final_name],
                "restore previous Tart base",
            );
        }
        return Err(error);
    }

    if had_previous {
        delete(tart, &backup);
    }
    Ok(())
}

pub(super) fn exists(tart: &TartCommand, name: &str) -> VmResult<bool> {
    let output = tart
        .command()
        .args(["list", "--format", "json"])
        .output()
        .map_err(|error| VmError::general(error, "Failed to list Tart bases"))?;
    if !output.status.success() {
        return Err(VmError::validation(
            "Failed to list Tart bases",
            Some("Run `tart list` to diagnose the Tart installation"),
        ));
    }
    list_contains(&output.stdout, name)
}

fn list_contains(output: &[u8], name: &str) -> VmResult<bool> {
    let entries: Vec<serde_json::Value> = serde_json::from_slice(output)?;
    Ok(entries
        .iter()
        .any(|entry| entry.get("Name").and_then(|value| value.as_str()) == Some(name)))
}

fn temporary_name(base_name: &str, role: &str) -> String {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    format!("{base_name}-{role}-{}", &suffix[..8])
}

fn run_tart(tart: &TartCommand, args: &[&str], context: &str) -> VmResult<()> {
    let status = tart
        .command()
        .args(args)
        .status()
        .map_err(|error| VmError::general(error, format!("Failed to {context}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(VmError::validation(
            format!("Failed to {context}: {status}"),
            None::<String>,
        ))
    }
}

fn run_host(mut command: Command, context: &str) -> VmResult<()> {
    let status = command
        .status()
        .map_err(|error| VmError::general(error, format!("Failed to {context}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(VmError::validation(
            format!("{context} failed with {status}"),
            None::<String>,
        ))
    }
}

fn delete(tart: &TartCommand, name: &str) {
    let _ = tart
        .command()
        .args(["delete", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn write_receipt(
    tart: &TartCommand,
    base_name: &str,
    guest_os: &str,
    source: &str,
) -> VmResult<()> {
    let path = receipt_path(base_name, tart.home())?;
    let receipt = BaseReceipt {
        name: base_name.to_string(),
        guest_os: guest_os.to_string(),
        controller_version: env!("CARGO_PKG_VERSION").to_string(),
        source: source.to_string(),
        tart_home: tart.home().map(Path::to_path_buf),
    };
    let mut content = serde_json::to_vec_pretty(&receipt)?;
    content.push(b'\n');
    vm_core::file_system::atomic_write(&path, &content)?;
    vm_core::file_system::set_permissions_mode(&path, 0o600).map_err(VmError::from)
}

fn receipt_path(base_name: &str, tart_home: Option<&Path>) -> VmResult<PathBuf> {
    let name = receipt_file_name(base_name, tart_home)?;
    let directory = vm_core::user_paths::vm_state_dir()?
        .join("tart")
        .join("bases");
    fs::create_dir_all(&directory)?;
    vm_core::file_system::set_permissions_mode(&directory, 0o700).map_err(VmError::from)?;
    Ok(directory.join(name))
}

fn receipt_file_name(base_name: &str, tart_home: Option<&Path>) -> VmResult<String> {
    if !base_name
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(VmError::validation(
            "Managed Tart base name contains unsupported characters",
            None::<String>,
        ));
    }
    let context = tart_home.map_or_else(
        || "default".to_string(),
        |path| format!("{:016x}", fnv1a(path.to_string_lossy().as_bytes())),
    );
    Ok(format!("{base_name}-{context}.json"))
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

#[cfg(test)]
mod tests {
    use super::{fnv1a, list_contains, receipt_file_name, temporary_name};

    #[test]
    fn temporary_names_are_unique_and_scoped() {
        let first = temporary_name("vibe-tart-linux-base", "staging");
        let second = temporary_name("vibe-tart-linux-base", "staging");
        assert!(first.starts_with("vibe-tart-linux-base-staging-"));
        assert_ne!(first, second);
    }

    #[test]
    fn receipt_name_cannot_escape_its_directory() {
        assert!(receipt_file_name("../../outside", None).is_err());
    }

    #[test]
    fn list_matching_uses_exact_base_name() {
        let output = br#"[
            {"Name":"vibe-tart-sequoia-base","State":"stopped"},
            {"Name":"vm-mac","State":"stopped"}
        ]"#;
        assert!(list_contains(output, "vibe-tart-sequoia-base").unwrap());
        assert!(!list_contains(output, "vibe-tart-linux-base").unwrap());
    }

    #[test]
    fn storage_contexts_have_distinct_stable_receipts() {
        let default = receipt_file_name("vibe-tart-linux-base", None).unwrap();
        let external = receipt_file_name(
            "vibe-tart-linux-base",
            Some(std::path::Path::new("/Volumes/Tart")),
        )
        .unwrap();
        assert_ne!(default, external);
        assert_eq!(fnv1a(b"same"), fnv1a(b"same"));
    }
}
