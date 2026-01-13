use crate::cli::ProfileSubcommand;
use crate::error::{VmError, VmResult};
use vm_config::{config::VmConfig, ConfigOps};

pub fn handle_profile_command(command: &ProfileSubcommand) -> VmResult<()> {
    match command {
        ProfileSubcommand::SetDefault { name } => {
            let config = VmConfig::load(None).map_err(VmError::from)?;
            let has_profile = config
                .profiles
                .as_ref()
                .map(|profiles| profiles.contains_key(name))
                .unwrap_or(false);

            if !has_profile {
                return Err(VmError::config(
                    std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("Profile '{}' not found in vm.yaml", name),
                    ),
                    "Invalid default profile",
                ));
            }

            ConfigOps::set("default_profile", std::slice::from_ref(name), false, false)
                .map_err(VmError::from)
        }
    }
}
