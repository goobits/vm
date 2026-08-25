pub(super) fn check_permissions() -> Result<(), String> {
    let home = dirs::home_dir().ok_or_else(|| "Could not determine home directory".to_string())?;
    let ssh_dir = home.join(".ssh");
    if !ssh_dir.exists() {
        return Err("SSH directory doesn't exist".to_string());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mode = std::fs::metadata(&ssh_dir)
            .map_err(|error| format!("Cannot read SSH directory: {error}"))?
            .permissions()
            .mode()
            & 0o777;
        if mode != 0o700 {
            return Err(format!(
                "SSH directory has wrong permissions: {mode:o} (should be 700)"
            ));
        }
        for (key, label) in [("id_rsa", "SSH key"), ("id_ed25519", "SSH key (ed25519)")] {
            let path = ssh_dir.join(key);
            if !path.exists() {
                continue;
            }
            let mode = std::fs::metadata(&path)
                .map_err(|error| format!("Cannot read SSH key: {error}"))?
                .permissions()
                .mode()
                & 0o777;
            if mode != 0o600 {
                return Err(format!(
                    "{label} has wrong permissions: {mode:o} (should be 600)"
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn fix_permissions() -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let Some(ssh_dir) = dirs::home_dir().map(|home| home.join(".ssh")) else {
            return false;
        };
        if !ssh_dir.exists()
            || std::fs::set_permissions(&ssh_dir, std::fs::Permissions::from_mode(0o700)).is_err()
        {
            return false;
        }
        for key in ["id_rsa", "id_ed25519", "id_ecdsa"] {
            let path = ssh_dir.join(key);
            if path.exists() {
                let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
            }
        }
        true
    }

    #[cfg(not(unix))]
    {
        false
    }
}
