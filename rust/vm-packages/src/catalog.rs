use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{validate_label, validate_repository_url, PackageEcosystem, PackageValidationError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterPackage {
    pub name: String,
    pub ecosystem: PackageEcosystem,
    pub repository: String,
    #[serde(default = "default_branch")]
    pub default_branch: String,
}

impl RegisterPackage {
    pub fn validate(&self) -> Result<(), PackageValidationError> {
        validate_label("package", &self.name)?;
        validate_label("default branch", &self.default_branch)?;
        validate_repository_url(&self.repository)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageDefinition {
    pub name: String,
    pub ecosystem: PackageEcosystem,
    pub repository: String,
    pub default_branch: String,
    pub registered_at: DateTime<Utc>,
}

fn default_branch() -> String {
    "main".into()
}
