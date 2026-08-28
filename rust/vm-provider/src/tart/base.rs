mod install;

use vm_config::config::{ImageSpec, VmConfig};
use vm_core::error::{Result, VmError};

use super::TartCommand;

pub const LINUX_NAME: &str = "vibe-tart-linux-base";
pub const MACOS_NAME: &str = "vibe-tart-sequoia-base";
pub const LINUX_REGISTRY: &str = "ghcr.io/goobits/vm-tart-linux";
const BASE_BUILDER: &str = include_str!("../resources/scripts/build-vibe-tart-base.sh");
const VIBE_AI_TOOLS_INSTALLER: &str = include_str!("../resources/scripts/install-vibe-ai-tools.sh");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TartBaseSource {
    Current,
    Pulled,
    Built,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedTartBase {
    pub name: String,
    pub guest_os: &'static str,
    pub source: TartBaseSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TartVibeBase {
    guest_os: &'static str,
}

impl TartVibeBase {
    fn from_guest_os(guest_os: &str) -> Result<Self> {
        match guest_os {
            "linux" => Ok(Self { guest_os: "linux" }),
            "macos" => Ok(Self { guest_os: "macos" }),
            _ => Err(VmError::validation(
                "Invalid Tart guest OS",
                Some("Use linux or macos"),
            )),
        }
    }

    fn local_name(self) -> String {
        if self.guest_os == "linux" {
            versioned_cache_name()
        } else {
            MACOS_NAME.to_string()
        }
    }

    fn prebuilt_image(self) -> Option<String> {
        (self.guest_os == "linux").then(versioned_image)
    }
}

pub fn guest_os(name: &str) -> Option<&'static str> {
    match name {
        LINUX_NAME => Some("linux"),
        MACOS_NAME => Some("macos"),
        _ if name
            .strip_prefix(LINUX_NAME)
            .is_some_and(|suffix| suffix.starts_with("-v")) =>
        {
            Some("linux")
        }
        _ => None,
    }
}

pub fn versioned_image() -> String {
    format!("{LINUX_REGISTRY}:v{}", env!("CARGO_PKG_VERSION"))
}

pub fn versioned_cache_name() -> String {
    format!("{LINUX_NAME}-v{}", env!("CARGO_PKG_VERSION"))
}

/// Build and atomically activate the managed Tart base for one guest OS.
pub fn build_vibe_base(config: Option<&VmConfig>, guest_os: &str) -> Result<String> {
    let base = TartVibeBase::from_guest_os(guest_os)?;
    let local_name = base.local_name();
    let command = TartCommand::from_config(config);
    install::build(&command, base.guest_os, &local_name, BASE_BUILDER)?;
    Ok(local_name)
}

/// Prepare a configured managed Tart base, if the configuration selects one.
pub fn ensure_configured_vibe_base(config: &VmConfig) -> Result<Option<PreparedTartBase>> {
    let Some(base) = configured_vibe_base(config) else {
        return Ok(None);
    };

    let command = TartCommand::from_config(Some(config));
    ensure_vibe_base(base, &command).map(Some)
}

fn configured_vibe_base(config: &VmConfig) -> Option<TartVibeBase> {
    if config.provider.as_deref() != Some("tart") {
        return None;
    }

    let ImageSpec::String(name) = config.vm.as_ref()?.image.clone()? else {
        return None;
    };

    match name.as_str() {
        LINUX_NAME => Some(TartVibeBase { guest_os: "linux" }),
        MACOS_NAME => Some(TartVibeBase { guest_os: "macos" }),
        _ => None,
    }
}

fn ensure_vibe_base(base: TartVibeBase, command: &TartCommand) -> Result<PreparedTartBase> {
    let local_name = base.local_name();

    if install::exists(command, &local_name)?
        && install::receipt_matches(command, &local_name, base.guest_os)?
    {
        return Ok(PreparedTartBase {
            name: local_name,
            guest_os: base.guest_os,
            source: TartBaseSource::Current,
        });
    }

    if let Some(image) = base.prebuilt_image() {
        if install::pull(command, &image, &local_name, base.guest_os)? {
            return Ok(PreparedTartBase {
                name: local_name,
                guest_os: base.guest_os,
                source: TartBaseSource::Pulled,
            });
        }
    }

    install::build(command, base.guest_os, &local_name, BASE_BUILDER).map_err(|error| {
        VmError::validation(
            format!("Could not prepare Tart vibe base '{local_name}': {error}"),
            Some("Run `vm system base build vibe --provider tart` to retry with full output"),
        )
    })?;
    Ok(PreparedTartBase {
        name: local_name,
        guest_os: base.guest_os,
        source: TartBaseSource::Built,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use vm_config::config::{TartConfig, VmSettings};

    fn config(provider: &str, image_name: &str) -> VmConfig {
        VmConfig {
            provider: Some(provider.into()),
            vm: Some(VmSettings {
                image: Some(ImageSpec::String(image_name.to_string())),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn metadata_has_one_versioned_owner() {
        assert_eq!(guest_os(LINUX_NAME), Some("linux"));
        assert_eq!(guest_os(MACOS_NAME), Some("macos"));
        assert_eq!(guest_os(&versioned_cache_name()), Some("linux"));
        assert_eq!(guest_os("custom"), None);
        assert_eq!(
            versioned_image(),
            format!("{LINUX_REGISTRY}:v{}", env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(
            versioned_cache_name(),
            format!("{LINUX_NAME}-v{}", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn explicit_guest_os_resolves_base_name() {
        assert_eq!(
            TartVibeBase::from_guest_os("linux").unwrap().local_name(),
            versioned_cache_name()
        );
        assert_eq!(
            TartVibeBase::from_guest_os("macos").unwrap().local_name(),
            MACOS_NAME
        );
        assert!(TartVibeBase::from_guest_os("windows").is_err());
    }

    #[test]
    fn base_builder_is_embedded_in_the_provider() {
        assert!(BASE_BUILDER.starts_with("#!/usr/bin/env bash"));
        assert!(BASE_BUILDER.contains("tart clone"));
        assert!(BASE_BUILDER.contains("--guest-os"));
        assert!(!BASE_BUILDER.contains("tart delete \"$BASE_NAME\""));
        assert!(BASE_BUILDER.contains("VIBE_AI_TOOLS_INSTALLER"));
        assert!(BASE_BUILDER.contains("antigravity claude codex"));

        for installer in [
            "https://antigravity.google/cli/install.sh",
            "https://claude.ai/install.sh",
            "https://chatgpt.com/codex/install.sh",
        ] {
            assert!(VIBE_AI_TOOLS_INSTALLER.contains(installer));
        }
        for runtime_contract in [
            "codex-package.json",
            "cp -R \"$codex_package_dir/.\"",
            "/usr/local/lib/vm-ai-tools/codex-package/bin/codex",
            "/usr/local/lib/vm-ai-tools/codex-package/bin/codex-code-mode-host",
        ] {
            assert!(VIBE_AI_TOOLS_INSTALLER.contains(runtime_contract));
        }
    }

    #[test]
    fn configured_bases_resolve_their_guest_os() {
        assert_eq!(
            configured_vibe_base(&config("tart", MACOS_NAME)),
            Some(TartVibeBase { guest_os: "macos" })
        );
        assert_eq!(
            configured_vibe_base(&config("tart", LINUX_NAME)),
            Some(TartVibeBase { guest_os: "linux" })
        );
        assert_eq!(
            configured_vibe_base(&config("tart", LINUX_NAME))
                .and_then(TartVibeBase::prebuilt_image),
            Some(versioned_image())
        );
        assert_eq!(configured_vibe_base(&config("tart", "custom")), None);
        assert_eq!(configured_vibe_base(&config("docker", MACOS_NAME)), None);
    }

    #[test]
    fn tart_storage_path_is_forwarded_to_commands() {
        let config = VmConfig {
            tart: Some(TartConfig {
                storage_path: Some("/Volumes/ExternalSSD/Tart".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let command = TartCommand::from_config(Some(&config)).command();
        assert!(command.get_envs().any(|(key, value)| key == "TART_HOME"
            && value == Some(OsStr::new("/Volumes/ExternalSSD/Tart"))));
    }
}
