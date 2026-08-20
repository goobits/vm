use std::fmt;
use std::ops::Deref;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Provider name as it appears in `vm.yaml`.
///
/// Unknown values remain representable so configuration validation can return
/// the same focused error instead of failing during YAML deserialization.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProviderName {
    #[default]
    Docker,
    Podman,
    Tart,
    Mock,
    Unknown(String),
}

impl ProviderName {
    pub const SUPPORTED: [&'static str; 3] = ["docker", "podman", "tart"];

    pub fn as_str(&self) -> &str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
            Self::Tart => "tart",
            Self::Mock => "mock",
            Self::Unknown(name) => name,
        }
    }

    pub fn is_container(&self) -> bool {
        matches!(self, Self::Docker | Self::Podman)
    }

    pub fn is_supported(&self) -> bool {
        matches!(self, Self::Docker | Self::Podman | Self::Tart)
            || cfg!(feature = "test-helpers") && matches!(self, Self::Mock)
    }
}

impl Deref for ProviderName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl fmt::Display for ProviderName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl From<&str> for ProviderName {
    fn from(name: &str) -> Self {
        match name {
            "docker" => Self::Docker,
            "podman" => Self::Podman,
            "tart" => Self::Tart,
            "mock" => Self::Mock,
            other => Self::Unknown(other.to_string()),
        }
    }
}

impl From<String> for ProviderName {
    fn from(name: String) -> Self {
        match name.as_str() {
            "docker" => Self::Docker,
            "podman" => Self::Podman,
            "tart" => Self::Tart,
            "mock" => Self::Mock,
            _ => Self::Unknown(name),
        }
    }
}

impl FromStr for ProviderName {
    type Err = std::convert::Infallible;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Ok(Self::from(name))
    }
}

impl Serialize for ProviderName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ProviderName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::from)
    }
}

#[cfg(test)]
mod tests {
    use super::ProviderName;

    #[test]
    fn serializes_without_changing_vm_yaml() {
        for name in ["docker", "podman", "tart", "future-engine"] {
            let provider = ProviderName::from(name);
            assert_eq!(serde_yaml_ng::to_string(&provider).unwrap().trim(), name);
            assert_eq!(
                serde_yaml_ng::from_str::<ProviderName>(name).unwrap(),
                provider
            );
        }
    }

    #[test]
    fn classifies_container_engines() {
        assert!(ProviderName::Docker.is_container());
        assert!(ProviderName::Podman.is_container());
        assert!(!ProviderName::Tart.is_container());
    }
}
