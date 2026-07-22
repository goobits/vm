// Shared parsing logic for memory and CPU resource limits
//
// This module provides unified parsing for resource limit values across different formats:
// - Raw numbers: 1024, 4
// - Memory units: "1gb", "4g", "512mb", "512m"
// - Percentages: "50%", "90%"
// - Unlimited: "unlimited"

use serde::de;
use std::fmt;

/// Parsed limit value before conversion to specific limit type
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedLimit {
    /// Raw number (meaning depends on context: MB for memory, count for CPU)
    Number(u32),
    /// Memory size with unit (bytes)
    Bytes(u64),
    /// Percentage of available resources (1-100)
    Percentage(u8),
    /// No limit
    Unlimited,
}

/// Parse a resource limit value from string
///
/// # Supported Formats
/// - Numbers: "1024", "4"
/// - Memory units: "1gb", "4g", "512mb", "512m", "2GB" (case-insensitive)
/// - Percentages: "50%", "90%", "75%"
/// - Unlimited: "unlimited"
///
/// # Examples
/// ```
/// # use vm_config::limit_parser::{parse_limit_value, ParsedLimit};
/// assert_eq!(parse_limit_value("1024").unwrap(), ParsedLimit::Number(1024));
/// assert_eq!(parse_limit_value("1gb").unwrap(), ParsedLimit::Bytes(1024 * 1024 * 1024));
/// assert_eq!(parse_limit_value("50%").unwrap(), ParsedLimit::Percentage(50));
/// assert_eq!(parse_limit_value("unlimited").unwrap(), ParsedLimit::Unlimited);
/// ```
pub fn parse_limit_value(s: &str) -> Result<ParsedLimit, String> {
    let s = s.trim();

    // Check for unlimited
    if s.eq_ignore_ascii_case("unlimited") {
        return Ok(ParsedLimit::Unlimited);
    }

    // Check for percentage
    if let Some(percent_str) = s.strip_suffix('%') {
        let percent = percent_str
            .trim()
            .parse::<u8>()
            .map_err(|_| format!("Invalid percentage value: '{}'", s))?;

        if percent == 0 || percent > 100 {
            return Err(format!(
                "Percentage must be between 1 and 100, got: {}",
                percent
            ));
        }

        return Ok(ParsedLimit::Percentage(percent));
    }

    // Check for binary memory units. Compose-style single-letter suffixes are
    // accepted alongside the more explicit forms.
    let lower = s.to_lowercase();
    for (suffix, multiplier) in [
        ("gb", 1024.0 * 1024.0 * 1024.0),
        ("g", 1024.0 * 1024.0 * 1024.0),
        ("mb", 1024.0 * 1024.0),
        ("m", 1024.0 * 1024.0),
        ("kb", 1024.0),
        ("k", 1024.0),
    ] {
        if let Some(number) = lower.strip_suffix(suffix) {
            return Ok(ParsedLimit::Bytes(
                (parse_float(number.trim())? * multiplier) as u64,
            ));
        }
    }

    // Try parsing as raw number
    let num = s
        .parse::<u32>()
        .map_err(|_| format!("Invalid limit value: '{}'", s))?;

    Ok(ParsedLimit::Number(num))
}

/// Parse a floating point number from string
fn parse_float(s: &str) -> Result<f64, String> {
    s.parse::<f64>()
        .map_err(|_| format!("Invalid numeric value: '{}'", s))
}

/// Serde visitor for deserializing limit values
///
/// Accepts:
/// - u32/u64: Direct numeric values
/// - String: "unlimited", "1gb", "50%", etc.
pub struct LimitVisitor<'a> {
    resource_name: &'a str,
}

impl<'a> LimitVisitor<'a> {
    pub fn new(resource_name: &'a str) -> Self {
        Self { resource_name }
    }
}

impl<'de, 'a> de::Visitor<'de> for LimitVisitor<'a> {
    type Value = ParsedLimit;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(
            formatter,
            "a positive integer ({}), memory size (e.g., \"1gb\", \"512mb\"), percentage (e.g., \"50%\"), or \"unlimited\"",
            self.resource_name
        )
    }

    fn visit_u32<E>(self, value: u32) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(ParsedLimit::Number(value))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if value <= u32::MAX as u64 {
            Ok(ParsedLimit::Number(value as u32))
        } else {
            Err(E::custom(format!(
                "{} limit too large (max: {})",
                self.resource_name,
                u32::MAX
            )))
        }
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        parse_limit_value(value).map_err(E::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_raw_numbers() {
        assert_eq!(
            parse_limit_value("1024").unwrap(),
            ParsedLimit::Number(1024)
        );
        assert_eq!(parse_limit_value("4").unwrap(), ParsedLimit::Number(4));
        assert_eq!(parse_limit_value("0").unwrap(), ParsedLimit::Number(0));
    }

    #[test]
    fn test_parse_memory_units() {
        // GB
        assert_eq!(
            parse_limit_value("1gb").unwrap(),
            ParsedLimit::Bytes(1024 * 1024 * 1024)
        );
        assert_eq!(
            parse_limit_value("2GB").unwrap(),
            ParsedLimit::Bytes(2 * 1024 * 1024 * 1024)
        );
        assert_eq!(
            parse_limit_value("1.5gb").unwrap(),
            ParsedLimit::Bytes((1.5 * 1024.0 * 1024.0 * 1024.0) as u64)
        );
        assert_eq!(
            parse_limit_value("4g").unwrap(),
            ParsedLimit::Bytes(4 * 1024 * 1024 * 1024)
        );

        // MB
        assert_eq!(
            parse_limit_value("512mb").unwrap(),
            ParsedLimit::Bytes(512 * 1024 * 1024)
        );
        assert_eq!(
            parse_limit_value("1024MB").unwrap(),
            ParsedLimit::Bytes(1024 * 1024 * 1024)
        );
        assert_eq!(
            parse_limit_value("512m").unwrap(),
            ParsedLimit::Bytes(512 * 1024 * 1024)
        );

        // KB
        assert_eq!(
            parse_limit_value("1024kb").unwrap(),
            ParsedLimit::Bytes(1024 * 1024)
        );
        assert_eq!(
            parse_limit_value("1024k").unwrap(),
            ParsedLimit::Bytes(1024 * 1024)
        );
    }

    #[test]
    fn test_parse_percentages() {
        assert_eq!(
            parse_limit_value("50%").unwrap(),
            ParsedLimit::Percentage(50)
        );
        assert_eq!(
            parse_limit_value("90%").unwrap(),
            ParsedLimit::Percentage(90)
        );
        assert_eq!(parse_limit_value("1%").unwrap(), ParsedLimit::Percentage(1));
        assert_eq!(
            parse_limit_value("100%").unwrap(),
            ParsedLimit::Percentage(100)
        );
    }

    #[test]
    fn test_parse_unlimited() {
        assert_eq!(
            parse_limit_value("unlimited").unwrap(),
            ParsedLimit::Unlimited
        );
        assert_eq!(
            parse_limit_value("UNLIMITED").unwrap(),
            ParsedLimit::Unlimited
        );
        assert_eq!(
            parse_limit_value("Unlimited").unwrap(),
            ParsedLimit::Unlimited
        );
    }

    #[test]
    fn test_parse_with_whitespace() {
        assert_eq!(
            parse_limit_value("  1024  ").unwrap(),
            ParsedLimit::Number(1024)
        );
        assert_eq!(
            parse_limit_value("  50%  ").unwrap(),
            ParsedLimit::Percentage(50)
        );
        assert_eq!(
            parse_limit_value("  unlimited  ").unwrap(),
            ParsedLimit::Unlimited
        );
    }

    #[test]
    fn test_parse_invalid_percentage() {
        assert!(parse_limit_value("0%").is_err());
        assert!(parse_limit_value("101%").is_err());
        assert!(parse_limit_value("200%").is_err());
    }

    #[test]
    fn test_parse_invalid_values() {
        assert!(parse_limit_value("invalid").is_err());
        assert!(parse_limit_value("1.5").is_err()); // Decimal without unit
        assert!(parse_limit_value("-10").is_err());
        assert!(parse_limit_value("10tb").is_err()); // Unsupported unit
    }

    #[test]
    fn test_parse_edge_cases() {
        // Empty string
        assert!(parse_limit_value("").is_err());

        // Just units
        assert!(parse_limit_value("gb").is_err());
        assert!(parse_limit_value("mb").is_err());

        // Just percent sign
        assert!(parse_limit_value("%").is_err());
    }
}
