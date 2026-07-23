//! Sudo-aware home directory resolution.

use std::fs;
use std::path::PathBuf;

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

fn home_dir_from_passwd(user: &str) -> Option<PathBuf> {
    let contents = fs::read_to_string("/etc/passwd").ok()?;

    for line in contents.lines() {
        let mut parts = line.trim().split(':');
        let name = parts.next()?;
        if name != user {
            continue;
        }

        parts.next()?;
        parts.next()?;
        parts.next()?;
        parts.next()?;
        let home = parts.next()?;
        if !home.is_empty() {
            return Some(PathBuf::from(home));
        }
    }

    None
}
