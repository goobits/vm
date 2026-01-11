//! Environment variable management commands

use crate::cli::EnvSubcommand;
use crate::error::{VmError, VmResult};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use vm_config::AppConfig;
use vm_core::vm_println;

/// Handle environment variable commands
pub fn handle_env_command(
    command: &EnvSubcommand,
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
    let app_config = AppConfig::load(config_path, profile)?;

    match command {
        EnvSubcommand::Validate { all } => handle_validate(&app_config, *all),
        EnvSubcommand::Diff => handle_diff(&app_config),
        EnvSubcommand::List { show_values } => handle_list(&app_config, *show_values),
    }
}

/// Validate .env against template
fn handle_validate(app_config: &AppConfig, show_all: bool) -> VmResult<()> {
    let template_path = app_config
        .vm
        .project
        .as_ref()
        .and_then(|p| p.env_template_path.as_ref())
        .ok_or_else(|| {
            VmError::general(
                std::io::Error::new(std::io::ErrorKind::NotFound, "No template configured"),
                "No env_template_path configured in vm.yaml".to_string(),
            )
        })?;

    vm_println!("🔍 Validating environment variables\n");
    vm_println!("   Template: {}", template_path);

    // Parse template
    let template_vars = parse_env_file(template_path)?;
    vm_println!("   Found {} variables in template\n", template_vars.len());

    // Parse .env if it exists
    let env_path = ".env";
    let env_vars = if fs::metadata(env_path).is_ok() {
        parse_env_file(env_path)?
    } else {
        vm_println!("⚠️  No .env file found\n");
        HashMap::new()
    };

    // Find missing and extra variables
    let template_keys: HashSet<_> = template_vars.keys().collect();
    let env_keys: HashSet<_> = env_vars.keys().collect();

    let missing: Vec<_> = template_keys
        .difference(&env_keys)
        .map(|s| s.to_string())
        .collect();
    let extra: Vec<_> = env_keys
        .difference(&template_keys)
        .map(|s| s.to_string())
        .collect();

    let mut has_issues = false;

    // Show missing variables
    if !missing.is_empty() {
        has_issues = true;
        vm_println!("❌ Missing variables ({}):", missing.len());
        for var in &missing {
            if let Some(value) = template_vars.get(var) {
                if value.is_empty() || value == "\"\"" || value == "''" {
                    vm_println!("   • {} (no default)", var);
                } else {
                    vm_println!("   • {} (default: {})", var, mask_value(value));
                }
            }
        }
        vm_println!("");
    }

    // Show extra variables (not necessarily an error)
    if !extra.is_empty() && show_all {
        vm_println!("ℹ️  Extra variables ({}):", extra.len());
        for var in &extra {
            vm_println!("   • {} (not in template)", var);
        }
        vm_println!("");
    }

    // Show present variables if --all flag
    if show_all {
        let present: Vec<_> = template_keys.intersection(&env_keys).collect();
        if !present.is_empty() {
            vm_println!("✓ Present variables ({}):", present.len());
            for var in present {
                vm_println!("   • {}", var);
            }
            vm_println!("");
        }
    }

    if has_issues {
        vm_println!("💡 Copy template values to .env:");
        vm_println!("   cp {} .env", template_path);
        Err(VmError::validation(
            format!("{} variables missing from .env", missing.len()),
            None::<String>,
        ))
    } else {
        vm_println!("✓ All template variables are present in .env\n");
        Ok(())
    }
}

/// Show differences between .env and template
fn handle_diff(app_config: &AppConfig) -> VmResult<()> {
    let template_path = app_config
        .vm
        .project
        .as_ref()
        .and_then(|p| p.env_template_path.as_ref())
        .ok_or_else(|| {
            VmError::general(
                std::io::Error::new(std::io::ErrorKind::NotFound, "No template configured"),
                "No env_template_path configured in vm.yaml".to_string(),
            )
        })?;

    vm_println!("🔍 Environment Differences\n");

    // Parse both files
    let template_vars = parse_env_file(template_path)?;
    let env_vars = parse_env_file(".env")?;

    let template_keys: HashSet<_> = template_vars.keys().collect();
    let env_keys: HashSet<_> = env_vars.keys().collect();

    // Missing in .env
    let missing: Vec<_> = template_keys
        .difference(&env_keys)
        .map(|s| s.to_string())
        .collect();
    if !missing.is_empty() {
        vm_println!("➖ In template, missing in .env:");
        for var in missing {
            vm_println!("   {}", var);
        }
        vm_println!("");
    }

    // Extra in .env
    let extra: Vec<_> = env_keys
        .difference(&template_keys)
        .map(|s| s.to_string())
        .collect();
    if !extra.is_empty() {
        vm_println!("➕ In .env, not in template:");
        for var in extra {
            vm_println!("   {}", var);
        }
        vm_println!("");
    }

    // Different values
    let common: Vec<_> = template_keys.intersection(&env_keys).collect();
    let mut differences = Vec::new();
    for var in common {
        // These keys are guaranteed to exist because they're in the intersection
        if let (Some(template_val), Some(env_val)) = (template_vars.get(*var), env_vars.get(*var)) {
            if template_val != env_val {
                differences.push(var.to_string());
            }
        }
    }

    if !differences.is_empty() {
        vm_println!("🔄 Different values:");
        for var in differences {
            // These keys are guaranteed to exist because they're in both maps
            if let (Some(template_val), Some(env_val)) =
                (template_vars.get(&var), env_vars.get(&var))
            {
                vm_println!(
                    "   {} (template: {} → env: {})",
                    var,
                    mask_value(template_val),
                    mask_value(env_val)
                );
            }
        }
        vm_println!("");
    }

    Ok(())
}

/// List all environment variables
fn handle_list(app_config: &AppConfig, show_values: bool) -> VmResult<()> {
    let env_path = ".env";

    if fs::metadata(env_path).is_err() {
        vm_println!("❌ No .env file found\n");
        vm_println!("💡 Create one from template:");
        if let Some(template) = app_config
            .vm
            .project
            .as_ref()
            .and_then(|p| p.env_template_path.as_ref())
        {
            vm_println!("   cp {} .env\n", template);
        }
        return Ok(());
    }

    vm_println!("📋 Environment Variables\n");

    let env_vars = parse_env_file(env_path)?;
    let mut vars: Vec<_> = env_vars.iter().collect();
    vars.sort_by_key(|(k, _)| k.as_str());

    for (key, value) in vars {
        if show_values {
            vm_println!("   {}={}", key, mask_value(value));
        } else {
            vm_println!("   {} (hidden, use --show-values to display)", key);
        }
    }

    vm_println!("\n   Total: {} variables\n", env_vars.len());

    Ok(())
}

/// Parse .env file format
fn parse_env_file(path: &str) -> VmResult<HashMap<String, String>> {
    let content = fs::read_to_string(path)
        .map_err(|e| VmError::general(e, format!("Failed to read file: {}", path)))?;

    let mut vars = HashMap::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Parse KEY=VALUE
        if let Some(eq_pos) = trimmed.find('=') {
            let key = trimmed[..eq_pos].trim().to_string();
            let value = trimmed[eq_pos + 1..].trim().to_string();

            // Remove quotes if present
            let value = if (value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\''))
            {
                value[1..value.len() - 1].to_string()
            } else {
                value
            };

            vars.insert(key, value);
        }
    }

    Ok(vars)
}

/// Mask sensitive values
fn mask_value(value: &str) -> String {
    if value.is_empty() {
        return "(empty)".to_string();
    }

    if value.len() <= 4 {
        return "****".to_string();
    }

    format!("{}****", &value[..2])
}
