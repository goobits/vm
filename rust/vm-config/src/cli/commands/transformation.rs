use anyhow::Result;
use std::path::PathBuf;

use super::super::TransformFormat;

pub fn execute(file: PathBuf, expression: String, format: TransformFormat) -> Result<()> {
    use crate::yaml_ops::YamlOperations;
    YamlOperations::transform(&file, &expression, &format)
}