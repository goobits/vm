use anyhow::Result;
use std::path::PathBuf;

use super::super::OutputFormat;

pub fn execute_array_add(file: PathBuf, path: String, item: String) -> Result<()> {
    use crate::yaml::YamlOperations;
    YamlOperations::array_add(&file, &path, &item)
}

pub fn execute_array_remove(file: PathBuf, path: String, filter: String) -> Result<()> {
    use crate::yaml::YamlOperations;
    YamlOperations::array_remove(&file, &path, &filter)
}

pub fn execute_modify(file: PathBuf, field: String, value: String, stdout: bool) -> Result<()> {
    use crate::yaml::YamlOperations;
    YamlOperations::modify_file(&file, &field, &value, stdout)
}

pub fn execute_add_to_array(
    file: PathBuf,
    path: String,
    object: String,
    stdout: bool,
) -> Result<()> {
    use crate::yaml::YamlOperations;
    YamlOperations::add_to_array_path(&file, &path, &object, stdout)
}

pub fn execute_delete(
    file: PathBuf,
    path: String,
    field: String,
    value: String,
    format: OutputFormat,
) -> Result<()> {
    use crate::yaml::YamlOperations;
    YamlOperations::delete_from_array(&file, &path, &field, &value, &format)
}