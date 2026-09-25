//! Tart image and snapshot source selection, shared by creation and preview.

use std::path::Path;
use vm_config::config::{ImageSpec, VmConfig};
use vm_core::error::Result;

use crate::{tart_base, VmError};

const DEFAULT_TART_IMAGE: &str = "ghcr.io/cirruslabs/macos-sequoia-base:latest";

#[derive(Debug)]
pub(super) enum TartImageSource {
    Image(String),
    Snapshot(String),
}

impl TartImageSource {
    pub(super) fn parse(spec: &ImageSpec) -> Result<Self> {
        match spec {
            ImageSpec::String(value) => {
                if let Some(name) = value.strip_prefix('@') {
                    return Ok(Self::Snapshot(name.to_string()));
                }
                let lower = value.to_ascii_lowercase();
                if value.starts_with("./")
                    || value.starts_with("../")
                    || Path::new(value).is_absolute()
                    || lower == "dockerfile"
                    || lower.ends_with("/dockerfile")
                    || lower.ends_with(".dockerfile")
                {
                    return Err(VmError::Config(format!(
                        "'{value}' looks like a Dockerfile path, but the Tart provider cannot build Dockerfiles. Use provider: docker or choose a Tart OCI image."
                    )));
                }
                Ok(Self::Image(value.clone()))
            }
            ImageSpec::Build { .. } => Err(VmError::Config(
                "Tart provider does not support Dockerfile builds".to_string(),
            )),
        }
    }
}

pub(super) fn configured_source(config: &VmConfig) -> Result<TartImageSource> {
    let Some(spec) = config.vm.as_ref().and_then(|vm| vm.image.as_ref()) else {
        return Ok(TartImageSource::Image(DEFAULT_TART_IMAGE.to_string()));
    };
    match TartImageSource::parse(spec)? {
        TartImageSource::Image(image) if image == tart_base::LINUX_NAME => {
            Ok(TartImageSource::Image(tart_base::versioned_cache_name()))
        }
        source => Ok(source),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tart_images_and_snapshots() {
        assert!(matches!(
            TartImageSource::parse(&ImageSpec::String("ghcr.io/example/tart:latest".into())).unwrap(),
            TartImageSource::Image(image) if image == "ghcr.io/example/tart:latest"
        ));
        assert!(matches!(
            TartImageSource::parse(&ImageSpec::String("@release".into())).unwrap(),
            TartImageSource::Snapshot(name) if name == "release"
        ));
    }

    #[test]
    fn rejects_dockerfile_sources() {
        for value in [
            "Dockerfile",
            "./Dockerfile",
            "../Dockerfile",
            "build.dev.dockerfile",
        ] {
            assert!(TartImageSource::parse(&ImageSpec::String(value.into()))
                .unwrap_err()
                .to_string()
                .contains("looks like a Dockerfile path"));
        }
        assert!(TartImageSource::parse(&ImageSpec::Build {
            dockerfile: "Dockerfile".into(),
            context: None,
            args: None
        })
        .is_err());
    }
}
