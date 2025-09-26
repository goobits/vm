//! Strong type definitions for package server identifiers
//!
//! This module provides type-safe wrappers around common string identifiers
//! to prevent mixing up package names, versions, and registry types.

use crate::validation::{ValidationError, ValidationResult};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// A validated package name
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PackageName(String);

/// A validated package version
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Version(String);

/// Registry type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Registry {
    Npm,
    Pypi,
    Cargo,
}

impl PackageName {
    /// Create a new PackageName with validation
    pub fn new(name: String) -> ValidationResult<Self> {
        if name.is_empty() {
            return Err(ValidationError::TooShort { actual: 0, min: 1 });
        }

        if name.len() > 214 {
            return Err(ValidationError::TooLong {
                actual: name.len(),
                max: 214,
            });
        }

        // Check for null bytes and control characters
        if name.contains('\0') {
            return Err(ValidationError::NullBytes);
        }

        if name.chars().any(|c| c.is_control()) {
            return Err(ValidationError::ControlCharacters);
        }

        Ok(PackageName(name))
    }

    /// Get the package name as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Version {
    /// Create a new Version with basic validation
    pub fn new(version: String) -> ValidationResult<Self> {
        if version.is_empty() {
            return Err(ValidationError::TooShort { actual: 0, min: 1 });
        }

        if version.len() > 64 {
            return Err(ValidationError::TooLong {
                actual: version.len(),
                max: 64,
            });
        }

        // Check for null bytes and control characters
        if version.contains('\0') {
            return Err(ValidationError::NullBytes);
        }

        if version.chars().any(|c| c.is_control()) {
            return Err(ValidationError::ControlCharacters);
        }

        Ok(Version(version))
    }

    /// Get the version as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Registry {
    /// Get the registry name as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            Registry::Npm => "npm",
            Registry::Pypi => "pypi",
            Registry::Cargo => "cargo",
        }
    }
}

// Display implementations
impl fmt::Display for PackageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for Registry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// FromStr implementations
impl FromStr for PackageName {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        PackageName::new(s.to_string())
    }
}

impl FromStr for Version {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Version::new(s.to_string())
    }
}

impl FromStr for Registry {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "npm" => Ok(Registry::Npm),
            "pypi" => Ok(Registry::Pypi),
            "cargo" => Ok(Registry::Cargo),
            _ => Err(ValidationError::InvalidFormat {
                reason: format!("Unknown registry type: {}", s),
            }),
        }
    }
}

// Conversion from String
impl TryFrom<String> for PackageName {
    type Error = ValidationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        PackageName::new(value)
    }
}

impl TryFrom<String> for Version {
    type Error = ValidationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Version::new(value)
    }
}

// Conversion from &str
impl TryFrom<&str> for PackageName {
    type Error = ValidationError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        PackageName::new(value.to_string())
    }
}

impl TryFrom<&str> for Version {
    type Error = ValidationError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Version::new(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_name_validation() {
        // Valid names
        assert!(PackageName::new("express".to_string()).is_ok());
        assert!(PackageName::new("@types/node".to_string()).is_ok());
        assert!(PackageName::new("my-package_name".to_string()).is_ok());

        // Invalid names
        assert!(PackageName::new("".to_string()).is_err()); // Empty
        assert!(PackageName::new("a".repeat(215)).is_err()); // Too long
        assert!(PackageName::new("test\0null".to_string()).is_err()); // Null byte
        assert!(PackageName::new("test\x01control".to_string()).is_err()); // Control char
    }

    #[test]
    fn test_version_validation() {
        // Valid versions
        assert!(Version::new("1.0.0".to_string()).is_ok());
        assert!(Version::new("2.1.3-beta.1".to_string()).is_ok());
        assert!(Version::new("1.0.0-alpha+build.1".to_string()).is_ok());

        // Invalid versions
        assert!(Version::new("".to_string()).is_err()); // Empty
        assert!(Version::new("v".repeat(65)).is_err()); // Too long
        assert!(Version::new("1.0\0.0".to_string()).is_err()); // Null byte
    }

    #[test]
    fn test_registry_parsing() {
        assert_eq!("npm".parse::<Registry>().unwrap(), Registry::Npm);
        assert_eq!("PYPI".parse::<Registry>().unwrap(), Registry::Pypi);
        assert_eq!("Cargo".parse::<Registry>().unwrap(), Registry::Cargo);
        assert!("invalid".parse::<Registry>().is_err());
    }

    #[test]
    fn test_registry_display() {
        assert_eq!(Registry::Npm.as_str(), "npm");
        assert_eq!(Registry::Pypi.as_str(), "pypi");
        assert_eq!(Registry::Cargo.as_str(), "cargo");
    }
}
