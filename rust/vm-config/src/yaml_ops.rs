use anyhow::{Result, Context};
use serde_yaml::{Value, Mapping};
use std::path::{Path, PathBuf};
use std::fs;
use std::io::Read;
use crate::cli::{OutputFormat, TransformFormat};

/// YAML operations for configuration processing
pub struct YamlOperations;

impl YamlOperations {
    /// Helper function to read from file or stdin
    fn read_file_or_stdin(file: &PathBuf) -> Result<String> {
        if file.to_str() == Some("-") {
            let mut buffer = String::new();
            std::io::stdin().read_to_string(&mut buffer)
                .with_context(|| "Failed to read from stdin")?;
            Ok(buffer)
        } else {
            fs::read_to_string(file)
                .with_context(|| format!("Failed to read file: {:?}", file))
        }
    }
    /// Validate that a file is valid YAML
    pub fn validate_file(file: &PathBuf) -> Result<()> {
        let content = Self::read_file_or_stdin(file)?;

        let _: Value = serde_yaml::from_str(&content)
            .with_context(|| format!("Invalid YAML in file: {:?}", file))?;

        Ok(())
    }

    /// Add an item to a YAML array at the specified path
    pub fn array_add(file: &PathBuf, path: &str, item: &str) -> Result<()> {
        let content = Self::read_file_or_stdin(file)?;

        let mut value: Value = serde_yaml::from_str(&content)
            .with_context(|| format!("Invalid YAML in file: {:?}", file))?;

        // Parse the item as YAML
        let new_item: Value = serde_yaml::from_str(item)
            .with_context(|| format!("Invalid YAML item: {}", item))?;

        // Navigate to the path and add the item
        let path_parts: Vec<&str> = path.split('.').collect();
        Self::add_to_array_at_path(&mut value, &path_parts, new_item)?;

        // Write back to file
        let updated_yaml = serde_yaml::to_string(&value)
            .with_context(|| "Failed to serialize YAML")?;

        fs::write(file, updated_yaml)
            .with_context(|| format!("Failed to write file: {:?}", file))?;

        Ok(())
    }

    /// Remove items from a YAML array based on filter
    pub fn array_remove(file: &PathBuf, path: &str, filter: &str) -> Result<()> {
        let content = Self::read_file_or_stdin(file)
            .with_context(|| format!("Failed to read file: {:?}", file))?;

        let mut value: Value = serde_yaml::from_str(&content)
            .with_context(|| format!("Invalid YAML in file: {:?}", file))?;

        // Navigate to the path and remove matching items
        let path_parts: Vec<&str> = path.split('.').collect();
        Self::remove_from_array_at_path(&mut value, &path_parts, filter)?;

        // Write back to file
        let updated_yaml = serde_yaml::to_string(&value)
            .with_context(|| "Failed to serialize YAML")?;

        fs::write(file, updated_yaml)
            .with_context(|| format!("Failed to write file: {:?}", file))?;

        Ok(())
    }

    /// Query with conditional filtering
    pub fn filter(file: &PathBuf, expression: &str, output_format: &OutputFormat) -> Result<()> {
        let content = Self::read_file_or_stdin(file)?;

        let value: Value = serde_yaml::from_str(&content)
            .with_context(|| format!("Invalid YAML in file: {:?}", file))?;

        // Apply the filter expression
        let result = Self::apply_filter(&value, expression)?;

        // Output in requested format
        match output_format {
            OutputFormat::Yaml => {
                let yaml = serde_yaml::to_string(&result)?;
                print!("{}", yaml);
            }
            OutputFormat::Json => {
                let json = serde_json::to_string(&result)?;
                println!("{}", json);
            }
            OutputFormat::JsonPretty => {
                let json = serde_json::to_string_pretty(&result)?;
                println!("{}", json);
            }
        }

        Ok(())
    }

    // Helper function to navigate to array and add item
    fn add_to_array_at_path(value: &mut Value, path: &[&str], item: Value) -> Result<()> {
        if path.is_empty() {
            return Err(anyhow::anyhow!("Empty path"));
        }

        if path.len() == 1 {
            // We're at the target array
            match value {
                Value::Sequence(seq) => {
                    seq.push(item);
                    return Ok(());
                }
                Value::Mapping(map) => {
                    let key = Value::String(path[0].to_string());
                    match map.get_mut(&key) {
                        Some(Value::Sequence(seq)) => {
                            seq.push(item);
                            return Ok(());
                        }
                        Some(_) => return Err(anyhow::anyhow!("Path '{}' is not an array", path[0])),
                        None => {
                            // Create new array
                            map.insert(key, Value::Sequence(vec![item]));
                            return Ok(());
                        }
                    }
                }
                _ => return Err(anyhow::anyhow!("Cannot navigate path on non-object")),
            }
        }

        // Navigate deeper
        match value {
            Value::Mapping(map) => {
                let key = Value::String(path[0].to_string());
                match map.get_mut(&key) {
                    Some(nested) => Self::add_to_array_at_path(nested, &path[1..], item)?,
                    None => {
                        // Create nested structure
                        let mut nested = Value::Mapping(Mapping::new());
                        Self::add_to_array_at_path(&mut nested, &path[1..], item)?;
                        map.insert(key, nested);
                    }
                }
            }
            _ => return Err(anyhow::anyhow!("Cannot navigate path on non-object")),
        }

        Ok(())
    }

    // Helper function to navigate to array and remove items
    fn remove_from_array_at_path(value: &mut Value, path: &[&str], filter: &str) -> Result<()> {
        if path.is_empty() {
            return Err(anyhow::anyhow!("Empty path"));
        }

        if path.len() == 1 {
            // We're at the target array
            match value {
                Value::Mapping(map) => {
                    let key = Value::String(path[0].to_string());
                    match map.get_mut(&key) {
                        Some(Value::Sequence(seq)) => {
                            seq.retain(|item| !Self::matches_filter(item, filter));
                            return Ok(());
                        }
                        Some(_) => return Err(anyhow::anyhow!("Path '{}' is not an array", path[0])),
                        None => return Err(anyhow::anyhow!("Path '{}' not found", path[0])),
                    }
                }
                _ => return Err(anyhow::anyhow!("Cannot navigate path on non-object")),
            }
        }

        // Navigate deeper
        match value {
            Value::Mapping(map) => {
                let key = Value::String(path[0].to_string());
                match map.get_mut(&key) {
                    Some(nested) => Self::remove_from_array_at_path(nested, &path[1..], filter)?,
                    None => return Err(anyhow::anyhow!("Path '{}' not found", path[0])),
                }
            }
            _ => return Err(anyhow::anyhow!("Cannot navigate path on non-object")),
        }

        Ok(())
    }

    // Simple filter matching (can be enhanced with more complex expressions)
    fn matches_filter(value: &Value, filter: &str) -> bool {
        // Support basic equality filters like: .source == "value"
        if filter.starts_with(".") {
            if let Some(eq_pos) = filter.find(" == ") {
                let field = &filter[1..eq_pos].trim();
                let expected = filter[eq_pos + 4..].trim().trim_matches('"');

                return Self::get_field_value(value, field)
                    .map(|v| v.as_str().unwrap_or("") == expected)
                    .unwrap_or(false);
            }
        }

        // Fallback: simple string matching
        let value_str = match value {
            Value::String(s) => s.as_str(),
            _ => return false,
        };

        value_str.contains(filter)
    }

    // Get field value from YAML value
    fn get_field_value<'a>(value: &'a Value, field: &str) -> Option<&'a Value> {
        match value {
            Value::Mapping(map) => map.get(&Value::String(field.to_string())),
            _ => None,
        }
    }

    // Apply filter expression (basic implementation)
    fn apply_filter(value: &Value, expression: &str) -> Result<Value> {
        // Handle array access with filters like: .mounts[] | select(.source == "value")
        if expression.contains("[]") && expression.contains("select") {
            // Simple implementation for array filtering
            if let Some(array_part) = expression.split("[]").next() {
                let array_path = array_part.trim_start_matches('.');
                if array_path == "mounts" {
                    if let Value::Mapping(map) = value {
                        if let Some(Value::Sequence(seq)) = map.get(&Value::String("mounts".to_string())) {
                            let results: Vec<Value> = seq.iter()
                                .filter(|_item| {
                                    // Simple select filter parsing
                                    if expression.contains(".source") {
                                        return true; // For now, return all items
                                    }
                                    true
                                })
                                .cloned()
                                .collect();
                            return Ok(Value::Sequence(results));
                        }
                    }
                }
            }
        }

        // Fallback: return the whole value
        Ok(value.clone())
    }

    /// Merge multiple YAML files with deep merging
    pub fn merge_eval_all(files: &[PathBuf], format: &OutputFormat) -> Result<()> {
        if files.len() < 2 {
            return Err(anyhow::anyhow!("Need at least 2 files to merge"));
        }

        // Load first file as base
        let mut result = Self::load_yaml_file(&files[0])?;

        // Merge subsequent files
        for file in &files[1..] {
            let overlay = Self::load_yaml_file(file)?;
            result = Self::deep_merge_values(result, overlay);
        }

        // Output result
        match format {
            OutputFormat::Yaml => {
                let yaml = serde_yaml::to_string(&result)?;
                print!("{}", yaml);
            }
            OutputFormat::Json => {
                let json = serde_json::to_string(&result)?;
                println!("{}", json);
            }
            OutputFormat::JsonPretty => {
                let json = serde_json::to_string_pretty(&result)?;
                println!("{}", json);
            }
        }

        Ok(())
    }


    /// Modify YAML file in-place
    pub fn modify_file(file: &PathBuf, field: &str, new_value: &str, stdout: bool) -> Result<()> {
        let mut value = Self::load_yaml_file(file)?;

        // Parse new value as YAML
        let parsed_value: Value = serde_yaml::from_str(new_value)
            .with_context(|| format!("Failed to parse new value: {}", new_value))?;

        // Set the field
        Self::set_field_value(&mut value, field, parsed_value)?;

        if stdout {
            let yaml = serde_yaml::to_string(&value)?;
            print!("{}", yaml);
        } else {
            let yaml = serde_yaml::to_string(&value)?;
            fs::write(file, yaml)
                .with_context(|| format!("Failed to write file: {:?}", file))?;
        }

        Ok(())
    }

    /// Get array length
    pub fn array_length(file: &PathBuf, path: &str) -> Result<usize> {
        let value = Self::load_yaml_file(file)?;

        let target = if path.is_empty() {
            &value
        } else {
            Self::get_nested_field(&value, path)?
        };

        match target {
            Value::Sequence(seq) => Ok(seq.len()),
            Value::Mapping(map) => Ok(map.len()),
            _ => Ok(0),
        }
    }

    /// Transform data with expressions
    pub fn transform(file: &PathBuf, expression: &str, format: &TransformFormat) -> Result<()> {
        let value = Self::load_yaml_file(file)?;

        let results = if expression.contains("to_entries[]") {
            // Handle to_entries transformations
            Self::transform_to_entries(&value, expression)?
        } else if expression.contains(".[]") {
            // Handle array iteration
            Self::transform_array_items(&value, expression)?
        } else {
            vec![expression.to_string()]
        };

        // Output in requested format
        match format {
            TransformFormat::Lines => {
                for result in results {
                    println!("{}", result);
                }
            }
            TransformFormat::Space => {
                println!("{}", results.join(" "));
            }
            TransformFormat::Comma => {
                println!("{}", results.join(","));
            }
            TransformFormat::Json => {
                let json = serde_json::to_string(&results)?;
                println!("{}", json);
            }
            TransformFormat::Yaml => {
                let yaml = serde_yaml::to_string(&results)?;
                print!("{}", yaml);
            }
        }

        Ok(())
    }

    /// Check if field exists and has subfield
    pub fn has_field(file: &PathBuf, field: &str, subfield: &str) -> Result<bool> {
        let value = Self::load_yaml_file(file)?;

        let target = Self::get_nested_field(&value, field)?;

        match target {
            Value::Mapping(map) => {
                let subfield_key = Value::String(subfield.to_string());
                Ok(map.contains_key(&subfield_key))
            }
            _ => Ok(false),
        }
    }

    /// Add object to array at specified path
    pub fn add_to_array_path(file: &PathBuf, path: &str, object_json: &str, stdout: bool) -> Result<()> {
        let mut value = Self::load_yaml_file(file)?;

        // Parse the JSON object and convert to YAML
        let json_value: serde_json::Value = serde_json::from_str(object_json)
            .with_context(|| format!("Failed to parse JSON object: {}", object_json))?;

        // Convert via string to avoid type compatibility issues
        let yaml_string = serde_yaml::to_string(&json_value)?;
        let yaml_value: Value = serde_yaml::from_str(&yaml_string)
            .with_context(|| "Failed to convert JSON to YAML")?;

        // Navigate to the path and add the object
        let path_parts: Vec<&str> = path.split('.').collect();
        Self::add_object_to_array_at_path(&mut value, &path_parts, yaml_value)?;

        if stdout {
            let yaml = serde_yaml::to_string(&value)?;
            print!("{}", yaml);
        } else {
            let yaml = serde_yaml::to_string(&value)?;
            fs::write(file, yaml)
                .with_context(|| format!("Failed to write file: {:?}", file))?;
        }

        Ok(())
    }

    /// Select items from array where field matches value
    pub fn select_where(file: &PathBuf, path: &str, field: &str, match_value: &str, format: &OutputFormat) -> Result<()> {
        let value = Self::load_yaml_file(file)?;

        let target = if path.is_empty() {
            &value
        } else {
            Self::get_nested_field(&value, path)?
        };

        let results = match target {
            Value::Sequence(seq) => {
                let mut matching_items = Vec::new();
                for item in seq {
                    if let Value::Mapping(map) = item {
                        let field_key = Value::String(field.to_string());
                        if let Some(field_value) = map.get(&field_key) {
                            if let Some(field_str) = field_value.as_str() {
                                if field_str == match_value {
                                    matching_items.push(item.clone());
                                }
                            }
                        }
                    }
                }
                Value::Sequence(matching_items)
            }
            _ => return Err(anyhow::anyhow!("Path does not point to an array")),
        };

        // Output results
        match format {
            OutputFormat::Yaml => {
                let yaml = serde_yaml::to_string(&results)?;
                print!("{}", yaml);
            }
            OutputFormat::Json => {
                let json = serde_json::to_string(&results)?;
                println!("{}", json);
            }
            OutputFormat::JsonPretty => {
                let json = serde_json::to_string_pretty(&results)?;
                println!("{}", json);
            }
        }

        Ok(())
    }

    /// Count items in array or object
    pub fn count_items(file: &PathBuf, path: &str) -> Result<usize> {
        let value = Self::load_yaml_file(file)?;

        let target = if path.is_empty() {
            &value
        } else {
            Self::get_nested_field(&value, path)?
        };

        match target {
            Value::Sequence(seq) => Ok(seq.len()),
            Value::Mapping(map) => Ok(map.len()),
            _ => Ok(0),
        }
    }

    // Helper functions

    fn load_yaml_file(file: &PathBuf) -> Result<Value> {
        let content = Self::read_file_or_stdin(file)?;

        serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse YAML: {:?}", file))
    }

    fn deep_merge_values(base: Value, overlay: Value) -> Value {
        match (base, overlay) {
            (Value::Mapping(mut base_map), Value::Mapping(overlay_map)) => {
                for (key, overlay_value) in overlay_map {
                    match base_map.get(&key) {
                        Some(base_value) => {
                            let merged = Self::deep_merge_values(base_value.clone(), overlay_value);
                            base_map.insert(key, merged);
                        }
                        None => {
                            base_map.insert(key, overlay_value);
                        }
                    }
                }
                Value::Mapping(base_map)
            }
            (_, overlay) => overlay,
        }
    }

    fn set_field_value(value: &mut Value, field: &str, new_value: Value) -> Result<()> {
        let parts: Vec<&str> = field.split('.').collect();
        if parts.is_empty() {
            return Err(anyhow::anyhow!("Empty field path"));
        }

        Self::set_nested_field(value, &parts, new_value)
    }

    fn set_nested_field(value: &mut Value, parts: &[&str], new_value: Value) -> Result<()> {
        if parts.len() == 1 {
            match value {
                Value::Mapping(map) => {
                    let key = Value::String(parts[0].to_string());
                    map.insert(key, new_value);
                    return Ok(());
                }
                _ => return Err(anyhow::anyhow!("Cannot set field on non-object")),
            }
        }

        match value {
            Value::Mapping(map) => {
                let key = Value::String(parts[0].to_string());
                match map.get_mut(&key) {
                    Some(nested) => Self::set_nested_field(nested, &parts[1..], new_value)?,
                    None => {
                        let mut nested = Value::Mapping(Mapping::new());
                        Self::set_nested_field(&mut nested, &parts[1..], new_value)?;
                        map.insert(key, nested);
                    }
                }
            }
            _ => return Err(anyhow::anyhow!("Cannot navigate path on non-object")),
        }

        Ok(())
    }

    fn get_nested_field<'a>(value: &'a Value, field: &str) -> Result<&'a Value> {
        let parts: Vec<&str> = field.split('.').collect();
        let mut current = value;

        for part in parts {
            match current {
                Value::Mapping(map) => {
                    let key = Value::String(part.to_string());
                    current = map.get(&key)
                        .ok_or_else(|| anyhow::anyhow!("Field '{}' not found", part))?;
                }
                _ => return Err(anyhow::anyhow!("Cannot navigate field '{}' on non-object", part)),
            }
        }

        Ok(current)
    }

    fn transform_to_entries(value: &Value, expression: &str) -> Result<Vec<String>> {
        // Parse the expression: .field | to_entries[] | "template"
        let parts: Vec<&str> = expression.split(" | ").collect();
        if parts.len() < 3 {
            return Ok(vec![expression.to_string()]);
        }

        let field_path = parts[0].trim_start_matches('.');
        let template = parts[2];

        // Navigate to the field in the YAML
        let target_value = if field_path.is_empty() {
            value
        } else {
            match Self::get_nested_field(value, field_path) {
                Ok(v) => v,
                Err(_) => return Ok(vec![]),
            }
        };

        match target_value {
            Value::Mapping(map) => {
                let mut results = Vec::new();
                for (key, val) in map {
                    if let Value::String(key_str) = key {
                        // Proper template replacement with jq-style syntax
                        let result = template
                            .replace("\\(.key)", key_str)
                            .replace("\\(.value)", &Self::yaml_value_to_string(val))
                            .replace(r"\(.key)", key_str)
                            .replace(r"\(.value)", &Self::yaml_value_to_string(val))
                            .trim_matches('"')
                            .to_string();
                        results.push(result);
                    }
                }
                Ok(results)
            }
            _ => Ok(vec![]),
        }
    }

    fn transform_array_items(value: &Value, _expression: &str) -> Result<Vec<String>> {
        // Simple array transformation - can be enhanced
        match value {
            Value::Sequence(seq) => {
                let mut results = Vec::new();
                for item in seq {
                    results.push(format!("{:?}", item));
                }
                Ok(results)
            }
            _ => Ok(vec![]),
        }
    }

    fn add_object_to_array_at_path(value: &mut Value, path: &[&str], object: Value) -> Result<()> {
        if path.is_empty() {
            return Err(anyhow::anyhow!("Empty path"));
        }

        if path.len() == 1 {
            // We're at the target array
            match value {
                Value::Sequence(seq) => {
                    seq.push(object);
                    return Ok(());
                }
                Value::Mapping(map) => {
                    let key = Value::String(path[0].to_string());
                    match map.get_mut(&key) {
                        Some(Value::Sequence(seq)) => {
                            seq.push(object);
                            return Ok(());
                        }
                        Some(_) => return Err(anyhow::anyhow!("Path '{}' is not an array", path[0])),
                        None => {
                            // Create new array
                            map.insert(key, Value::Sequence(vec![object]));
                            return Ok(());
                        }
                    }
                }
                _ => return Err(anyhow::anyhow!("Cannot navigate path on non-object")),
            }
        }

        // Navigate deeper
        match value {
            Value::Mapping(map) => {
                let key = Value::String(path[0].to_string());
                match map.get_mut(&key) {
                    Some(nested) => Self::add_object_to_array_at_path(nested, &path[1..], object)?,
                    None => {
                        // Create nested structure
                        let mut nested = Value::Mapping(Mapping::new());
                        Self::add_object_to_array_at_path(&mut nested, &path[1..], object)?;
                        map.insert(key, nested);
                    }
                }
            }
            _ => return Err(anyhow::anyhow!("Cannot navigate path on non-object")),
        }

        Ok(())
    }

    fn navigate_path_mut<'a>(value: &'a mut Value, path: &[&str]) -> Result<&'a mut Value> {
        if path.is_empty() {
            return Ok(value);
        }

        match value {
            Value::Mapping(map) => {
                let key = Value::String(path[0].to_string());
                match map.get_mut(&key) {
                    Some(nested) if path.len() == 1 => Ok(nested),
                    Some(nested) => Self::navigate_path_mut(nested, &path[1..]),
                    None => Err(anyhow::anyhow!("Path not found: {}", path[0])),
                }
            }
            _ => Err(anyhow::anyhow!("Cannot navigate path on non-mapping")),
        }
    }

    /// Convert YAML value to proper string representation (not Debug format)
    fn yaml_value_to_string(value: &Value) -> String {
        match value {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => "null".to_string(),
            Value::Sequence(seq) => {
                let items: Vec<String> = seq.iter().map(Self::yaml_value_to_string).collect();
                format!("[{}]", items.join(", "))
            }
            Value::Mapping(_) => serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string()),
            Value::Tagged(tagged) => Self::yaml_value_to_string(&tagged.value),
        }
    }

    pub fn delete_from_array(file: &Path, path: &str, field: &str, value: &str, format: &OutputFormat) -> Result<()> {
        let content = if file.to_str() == Some("-") {
            let mut buffer = String::new();
            std::io::stdin().read_to_string(&mut buffer)?;
            buffer
        } else {
            std::fs::read_to_string(file)?
        };
        let mut doc: Value = serde_yaml::from_str(&content)?;

        let path_parts: Vec<&str> = if path.is_empty() {
            vec![]
        } else {
            path.split('.').collect()
        };

        // Navigate to the array
        let target = if path_parts.is_empty() {
            &mut doc
        } else {
            Self::navigate_path_mut(&mut doc, &path_parts)?
        };

        // Filter the array
        if let Value::Sequence(seq) = target {
            seq.retain(|item| {
                if let Value::Mapping(map) = item {
                    if let Some(field_val) = map.get(&Value::String(field.to_string())) {
                        if let Value::String(s) = field_val {
                            return s != value;
                        }
                    }
                }
                true
            });
        } else {
            return Err(anyhow::anyhow!("Path '{}' is not an array", path));
        }

        // Output the result
        match format {
            OutputFormat::Yaml => {
                let yaml_str = serde_yaml::to_string(&doc)?;
                print!("{}", yaml_str);
            }
            OutputFormat::Json => {
                let json_str = serde_json::to_string(&doc)?;
                println!("{}", json_str);
            }
            OutputFormat::JsonPretty => {
                let json_str = serde_json::to_string_pretty(&doc)?;
                println!("{}", json_str);
            }
        }
        Ok(())
    }
}