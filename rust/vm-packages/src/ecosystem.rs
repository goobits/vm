use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

/// A package protocol supported by the infrastructure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageEcosystem {
    Npm,
    Cargo,
    Python,
}

impl PackageEcosystem {
    pub const ALL: [Self; 3] = [Self::Npm, Self::Cargo, Self::Python];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Cargo => "cargo",
            Self::Python => "python",
        }
    }
}

impl fmt::Display for PackageEcosystem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsePackageEcosystemError(String);

impl fmt::Display for ParsePackageEcosystemError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "unsupported package ecosystem '{}'; use npm, cargo, or python",
            self.0
        )
    }
}

impl std::error::Error for ParsePackageEcosystemError {}

impl FromStr for PackageEcosystem {
    type Err = ParsePackageEcosystemError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "npm" | "node" | "nodejs" => Ok(Self::Npm),
            "cargo" | "rust" => Ok(Self::Cargo),
            "python" | "pypi" | "pip" => Ok(Self::Python),
            _ => Err(ParsePackageEcosystemError(value.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PackageEcosystem;
    use std::str::FromStr;

    #[test]
    fn parses_protocol_aliases_to_one_domain_type() {
        assert_eq!(
            PackageEcosystem::from_str("nodejs").unwrap(),
            PackageEcosystem::Npm
        );
        assert_eq!(
            PackageEcosystem::from_str("rust").unwrap(),
            PackageEcosystem::Cargo
        );
        assert_eq!(
            PackageEcosystem::from_str("pypi").unwrap(),
            PackageEcosystem::Python
        );
    }
}
