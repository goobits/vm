// Standard library imports
use std::path::PathBuf;

// External crate imports
use vm_core::error::Result;

// Local module imports
use super::super::TransformFormat;

pub fn execute(file: PathBuf, expression: String, format: TransformFormat) -> Result<()> {
    use crate::yaml::YamlOperations;
    YamlOperations::transform(&file, &expression, &format)
}
