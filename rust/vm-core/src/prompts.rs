use std::io::IsTerminal;

use dialoguer::{theme::ColorfulTheme, MultiSelect, Select};

/// Show a shared arrow-key yes/no selector for interactive confirmations.
///
/// Returns the default in non-interactive contexts so callers can preserve
/// their existing non-TTY behavior.
pub fn confirm_select(prompt: &str, default: bool) -> Result<bool, dialoguer::Error> {
    if !std::io::stdin().is_terminal() {
        return Ok(default);
    }

    let options = ["Yes", "No"];
    let default_idx = usize::from(!default);
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .items(options)
        .default(default_idx)
        .interact()?;

    Ok(selection == 0)
}

/// Show a shared arrow-key selector and return the selected index.
pub fn select_index<T: std::fmt::Display>(
    prompt: &str,
    items: &[T],
    default_idx: usize,
) -> Result<usize, dialoguer::Error> {
    Select::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .items(items)
        .default(default_idx)
        .interact()
}

/// Show a space-toggle checklist and return selected item indexes.
///
/// Non-interactive callers receive an empty selection rather than blocking.
pub fn multi_select<T: std::fmt::Display>(
    prompt: &str,
    items: &[T],
    defaults: &[bool],
) -> Result<Vec<usize>, dialoguer::Error> {
    if !std::io::stdin().is_terminal() || items.is_empty() {
        return Ok(Vec::new());
    }

    MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .items(items)
        .defaults(defaults)
        .interact()
}

#[cfg(test)]
mod tests {
    #[test]
    fn non_tty_checklist_never_blocks() {
        if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
            assert!(super::multi_select("Select", &["one"], &[true])
                .unwrap()
                .is_empty());
        }
    }
}
