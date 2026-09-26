pub(crate) fn quote_posix_argument(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

/// Quote a path while preserving the explicit `$HOME` marker used by guest mounts.
#[cfg(any(feature = "tart", all(test, unix)))]
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
        "if [ -f {workspace}/.git ] && ! git -C {workspace} rev-parse --git-dir >/dev/null 2>&1; then git -C {workspace} worktree repair >/dev/null 2>&1 || true; fi"
    )
}

/// Prepare noninteractive commands without loading interactive shell hooks.
pub(crate) fn exec_script(workspace: &str, working_dir: &str) -> String {
    exec_script_with_profile(workspace, working_dir, "/etc/profile.d/vm-packages.sh")
}

fn exec_script_with_profile(workspace: &str, working_dir: &str, profile: &str) -> String {
    let repair = worktree_repair_script(workspace);
    let working_dir = quote_posix_argument(working_dir);
    let profile = quote_posix_argument(profile);
    format!(
        "{repair}\nif [ -r {profile} ]; then . {profile}; fi\nexport PATH=\"$HOME/.local/bin:$PATH\"\ncd {working_dir} && exec \"$@\""
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
            "if [ -f '/workspace/it'\"'\"'s here'/.git ] && ! git -C '/workspace/it'\"'\"'s here' rev-parse --git-dir >/dev/null 2>&1; then git -C '/workspace/it'\"'\"'s here' worktree repair >/dev/null 2>&1 || true; fi"
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
    fn noninteractive_exec_preserves_empty_and_shell_sensitive_arguments() {
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().to_str().unwrap();
        let script = exec_script_with_profile(workspace, workspace, "/nonexistent-vm-profile");
        let values = [
            "",
            "two words",
            "'quotes'",
            "$(touch injected)",
            "*",
            "line\nbreak",
        ];
        let output = std::process::Command::new("sh")
            .args(["-c", &script, "vm-exec", "printf", "%s\\0"])
            .args(values)
            .output()
            .unwrap();

        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let expected = values
            .iter()
            .flat_map(|value| value.bytes().chain([0]))
            .collect::<Vec<_>>();
        assert_eq!(output.stdout, expected);
        assert!(!directory.path().join("injected").exists());
    }

    #[cfg(unix)]
    #[test]
    fn noninteractive_exec_loads_managed_environment_and_preserves_exit_status() {
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().to_str().unwrap();
        let profile = directory.path().join("managed profile.sh");
        std::fs::write(&profile, "export VM_EXEC_TEST='managed settings'\n").unwrap();
        let script = exec_script_with_profile(workspace, workspace, profile.to_str().unwrap());
        let output = std::process::Command::new("sh")
            .args([
                "-c", &script, "vm-exec", "sh", "-c",
                "printf '%s' \"$VM_EXEC_TEST\"; case \"$PATH\" in \"$HOME/.local/bin:\"*) exit 17 ;; *) exit 18 ;; esac",
            ])
            .env("HOME", directory.path())
            .output()
            .unwrap();

        assert_eq!(output.status.code(), Some(17));
        assert_eq!(output.stdout, b"managed settings");
        assert!(output.stderr.is_empty());
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
