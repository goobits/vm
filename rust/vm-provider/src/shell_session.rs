pub(crate) fn quote_posix_argument(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

/// Quote a path while preserving the explicit `$HOME` marker used by guest mounts.
#[cfg(any(feature = "tart", test))]
pub(crate) fn quote_posix_home_path(value: &str) -> String {
    if value == "$HOME" {
        return r#""$HOME""#.to_string();
    }
    value.strip_prefix("$HOME/").map_or_else(
        || quote_posix_argument(value),
        |suffix| format!(r#""$HOME"/{}"#, quote_posix_argument(suffix)),
    )
}

pub(crate) fn worktree_repair_script(workspace: &str) -> String {
    let workspace = quote_posix_argument(workspace);
    format!(
        "if [ -e {workspace}/.git ]; then git -C {workspace} worktree repair >/dev/null 2>&1 || true; fi"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worktree_repair_uses_one_fully_quoted_argument() {
        let script = worktree_repair_script("/workspace/it's here");

        assert_eq!(
            script,
            "if [ -e '/workspace/it'\"'\"'s here'/.git ]; then git -C '/workspace/it'\"'\"'s here' worktree repair >/dev/null 2>&1 || true; fi"
        );
    }

    #[cfg(unix)]
    #[test]
    fn posix_argument_round_trips_through_sh() {
        let value = "spaces, 'quotes', and\na newline";
        let script = format!("set -- {}; printf %s \"$1\"", quote_posix_argument(value));
        let output = std::process::Command::new("sh")
            .args(["-c", &script])
            .output()
            .unwrap();

        assert!(output.status.success());
        assert_eq!(output.stdout, value.as_bytes());
    }

    #[cfg(unix)]
    #[test]
    fn guest_home_path_expands_only_the_home_marker() {
        let path = quote_posix_home_path("$HOME/config/it's here");
        let script = format!("printf %s {path}");
        let output = std::process::Command::new("sh")
            .args(["-c", &script])
            .env("HOME", "/guest/home")
            .output()
            .unwrap();

        assert!(output.status.success());
        assert_eq!(output.stdout, b"/guest/home/config/it's here");
    }
}
