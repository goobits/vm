pub(crate) fn quote_posix_argument(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
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
}
