use anyhow::{Context, Result};
use std::path::PathBuf;
use vm_common::vm_error;

use super::super::{utils::query_field, OutputFormat};
use crate::config::VmConfig;

pub fn execute_query(
    config: PathBuf,
    field: String,
    raw: bool,
    default: Option<String>,
) -> Result<()> {
    let config = VmConfig::from_file(&config)
        .with_context(|| format!("Failed to load config: {:?}", config))?;

    let json_value = serde_json::to_value(&config)?;
    let value = match query_field(&json_value, &field) {
        Ok(val) => {
            if val.is_null() && default.is_some() {
                serde_json::Value::String(
                    default
                        .ok_or_else(|| anyhow::anyhow!("Default value not available"))?,
                )
            } else {
                val
            }
        }
        Err(_) => {
            if let Some(default_val) = default {
                serde_json::Value::String(default_val)
            } else {
                return Err(anyhow::anyhow!("Field not found: {}", field));
            }
        }
    };

    if raw && value.is_string() {
        println!(
            "{}",
            value.as_str().ok_or_else(|| anyhow::anyhow!(
                "Expected string value, got: {:?}",
                value
            ))?
        );
    } else {
        println!("{}", serde_json::to_string(&value)?);
    }
    Ok(())
}

pub fn execute_filter(
    file: PathBuf,
    expression: String,
    output_format: OutputFormat,
) -> Result<()> {
    use crate::yaml::YamlOperations;
    YamlOperations::filter(&file, &expression, &output_format)
}

pub fn execute_array_length(file: PathBuf, path: String) -> Result<()> {
    use crate::yaml::YamlOperations;
    let length = YamlOperations::array_length(&file, &path)?;
    println!("{}", length);
    Ok(())
}

pub fn execute_has_field(file: PathBuf, field: String, subfield: String) -> Result<()> {
    use crate::yaml::YamlOperations;
    match YamlOperations::has_field(&file, &field, &subfield) {
        Ok(true) => {
            println!("true");
            std::process::exit(0);
        }
        Ok(false) => {
            println!("false");
            std::process::exit(1);
        }
        Err(e) => {
            vm_error!("Error checking field: {}", e);
            std::process::exit(1);
        }
    }
}

pub fn execute_select_where(
    file: PathBuf,
    path: String,
    field: String,
    value: String,
    format: OutputFormat,
) -> Result<()> {
    use crate::yaml::YamlOperations;
    YamlOperations::select_where(&file, &path, &field, &value, &format)
}

pub fn execute_count(file: PathBuf, path: String) -> Result<()> {
    use crate::yaml::YamlOperations;
    let count = YamlOperations::count_items(&file, &path)?;
    println!("{}", count);
    Ok(())
}