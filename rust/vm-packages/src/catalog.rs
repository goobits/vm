use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::PackageEcosystem;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterPackage {
    pub name: String,
    pub ecosystem: PackageEcosystem,
    pub repository: String,
    #[serde(default = "default_branch")]
    pub default_branch: String,
    #[serde(default)]
    pub ci_registry: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageDefinition {
    pub name: String,
    pub ecosystem: PackageEcosystem,
    pub repository: String,
    pub default_branch: String,
    #[serde(default)]
    pub ci_registry: Option<String>,
    pub registered_at: DateTime<Utc>,
}

fn default_branch() -> String {
    "main".into()
}
