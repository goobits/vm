use std::io::{self, Write};

use clap::CommandFactory;
use clap_complete::{generate, shells};

use crate::error::{VmError, VmResult};
use vm_core::vm_error;

const ZSH_PRELUDE: &str =
    "# Ensure compdef is available when this file is sourced directly from .zshrc.\n\
if [[ -n ${ZSH_VERSION:-} && -z ${functions[compdef]+x} ]]; then\n\
  autoload -Uz compinit\n\
  compinit -i\n\
fi\n\n";

pub(super) fn handle(shell: &str) -> VmResult<()> {
    let mut command = crate::cli::Args::command();
    let mut stdout = io::stdout();

    match shell.to_lowercase().as_str() {
        "bash" => generate(shells::Bash, &mut command, "vm", &mut stdout),
        "zsh" => {
            stdout.write_all(ZSH_PRELUDE.as_bytes())?;
            generate(shells::Zsh, &mut command, "vm", &mut stdout);
        }
        "fish" => generate(shells::Fish, &mut command, "vm", &mut stdout),
        "powershell" => generate(shells::PowerShell, &mut command, "vm", &mut stdout),
        _ => {
            vm_error!(
                "Unsupported shell: {}. Supported shells: bash, zsh, fish, powershell",
                shell
            );
            return Err(VmError::general(
                io::Error::new(io::ErrorKind::InvalidInput, "Unsupported shell"),
                format!("Shell '{shell}' is not supported. Use: bash, zsh, fish, or powershell"),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ZSH_PRELUDE;

    #[test]
    fn zsh_prelude_initializes_compdef_for_direct_sourcing() {
        assert!(ZSH_PRELUDE.contains("${functions[compdef]+x}"));
        assert!(ZSH_PRELUDE.contains("autoload -Uz compinit"));
        assert!(ZSH_PRELUDE.contains("compinit -i"));
    }
}
