use std::io::IsTerminal;

use dialoguer::{theme::ColorfulTheme, Select};

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
        .items(&options)
        .default(default_idx)
        .interact()?;

    Ok(selection == 0)
}

/// Show a shared arrow-key selector and return the selected index.
pub fn select_index(
    prompt: &str,
    items: &[impl ToString],
    default_idx: usize,
) -> Result<usize, dialoguer::Error> {
    Select::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .items(items)
        .default(default_idx)
        .interact()
}
