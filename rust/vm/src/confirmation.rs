//! One confirmation policy for destructive CLI operations.

use std::io::IsTerminal;

use crate::error::{VmError, VmResult};

pub fn destructive(prompt: &str, yes: bool) -> VmResult<bool> {
    if yes {
        return Ok(true);
    }
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return Err(VmError::validation(
            format!(
                "Confirmation requires an interactive terminal: {}",
                prompt.trim_end_matches('?')
            ),
            Some("Review the target, then repeat with --yes"),
        ));
    }
    vm_core::prompts::confirm_select(prompt, false).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use std::io::IsTerminal;

    #[test]
    fn nonterminal_confirmation_fails_instead_of_cancelling_silently() {
        if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
            let error = super::destructive("Delete resource?", false).unwrap_err();
            assert_eq!(error.exit_code(), 2);
            assert!(error.hint().unwrap().contains("--yes"));
        }
    }
}
