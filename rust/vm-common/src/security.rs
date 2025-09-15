use regex::Regex;

/// Validates that a string contains only safe characters for identifiers.
/// This is useful for validating container names, project names, etc.
pub fn validate_safe_identifier(
    identifier: &str,
    identifier_type: &str,
    allow_dashes: bool,
    allow_underscores: bool,
) -> Result<(), String> {
    if identifier.is_empty() {
        return Err(format!("Empty {} provided", identifier_type));
    }

    if identifier.len() > 64 {
        return Err(format!("{} is too long (max 64 characters)", identifier_type));
    }

    if identifier.len() < 1 {
        return Err(format!("{} is too short (min 1 character)", identifier_type));
    }

    let mut pattern = String::from("^[a-zA-Z0-9");
    if allow_dashes {
        pattern.push('-');
    }
    if allow_underscores {
        pattern.push('_');
    }
    pattern.push_str("]+$");

    let re = Regex::new(&pattern).unwrap();
    if !re.is_match(identifier) {
        let mut allowed = "alphanumeric characters".to_string();
        if allow_dashes && allow_underscores {
            allowed.push_str(", dashes, and underscores");
        } else if allow_dashes {
            allowed.push_str(" and dashes");
        } else if allow_underscores {
            allowed.push_str(" and underscores");
        }
        return Err(format!(
            "{} contains invalid characters. Only {} are allowed",
            identifier_type, allowed
        ));
    }

    if identifier.starts_with('-') || identifier.starts_with('_') {
        return Err(format!(
            "{} cannot start with a dash or underscore",
            identifier_type
        ));
    }

    Ok(())
}

/// Sanitize project name by removing potentially dangerous characters.
/// This function extracts only alphanumeric characters from a project name.
pub fn sanitize_project_name(project_name: &str) -> Result<String, String> {
    if project_name.is_empty() {
        return Err("Empty project name provided".to_string());
    }

    let sanitized_name: String = project_name.chars().filter(|c| c.is_alphanumeric()).collect();

    if sanitized_name.is_empty() {
        return Err(
            "Project name contains no valid characters after sanitization. Project names must contain at least one alphanumeric character.".to_string(),
        );
    }

    Ok(sanitized_name)
}
