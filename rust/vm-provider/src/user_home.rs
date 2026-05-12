//! Sudo-aware home directory resolution.
//!
//! When a CLI command is invoked via `sudo`, `$HOME` points at root's home and
//! `dirs::home_dir()` follows suit. For provider operations we usually want the
//! invoking user's home instead (so mounts, config writes, etc. land in the
//! right place). These helpers consult `$SUDO_USER` and `/etc/passwd` to recover
//! the real home directory in that case.

use std::fs;
use std::path::PathBuf;

/// Resolve the real user's home directory, accounting for sudo invocations.
pub(crate) fn resolve_home_dir() -> Option<PathBuf> {
    if let Ok(sudo_user) = std::env::var("SUDO_USER") {
        if !sudo_user.is_empty() && sudo_user != "root" {
            if let Some(home) = home_dir_from_passwd(&sudo_user) {
                return Some(home);
            }
        }
    }

    std::env::var("HOME").ok().map(PathBuf::from)
}

/// Look up a user's home directory from `/etc/passwd`.
pub(crate) fn home_dir_from_passwd(user: &str) -> Option<PathBuf> {
    let contents = fs::read_to_string("/etc/passwd").ok()?;

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let mut parts = line.split(':');
        let name = parts.next()?;
        if name != user {
            continue;
        }

        let _passwd = parts.next()?;
        let _uid = parts.next()?;
        let _gid = parts.next()?;
        let _gecos = parts.next()?;
        let home = parts.next()?;

        if !home.is_empty() {
            return Some(PathBuf::from(home));
        }
    }

    None
}
