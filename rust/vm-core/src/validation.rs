//! Centralized validation logic for user inputs and system interactions.
//!
//! This module provides shared validation functions to ensure consistency and security
//! across different parts of the VM tool.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::error::{Result, VmError};

/// Validate a server address (hostname or IP address)
///
/// Checks if the input is a valid hostname (RFC 1123) or IP address.
/// Prevents injection attacks and ensures format correctness.
///
/// # Arguments
/// * `server_addr` - The hostname or IP address to validate
///
/// # Returns
/// * `Ok(())` if valid
/// * `Err(VmError::Validation)` if invalid
pub fn validate_server_address(server_addr: &str) -> Result<()> {
    // Check basic length constraints
    if server_addr.is_empty() || server_addr.len() > 253 {
        return Err(VmError::Validation(
            "Server address must be between 1 and 253 characters".to_string(),
        ));
    }

    // Check for null bytes or control characters (injection prevention)
    if server_addr.contains('\0') || server_addr.chars().any(|c| c.is_control()) {
        return Err(VmError::Validation(
            "Server address contains invalid control characters".to_string(),
        ));
    }

    // 1. Try parsing as IP address
    if server_addr.parse::<IpAddr>().is_ok() {
        return Ok(());
    }

    // 2. Try parsing as IPv4 explicitly (catches some edge cases)
    if server_addr.parse::<Ipv4Addr>().is_ok() {
        return Ok(());
    }

    // 3. Try parsing as IPv6 explicitly
    // Handle bracketed IPv6 addresses if necessary, though std::net::IpAddr usually handles them
    let ipv6_candidate = if server_addr.starts_with('[') && server_addr.ends_with(']') {
        &server_addr[1..server_addr.len() - 1]
    } else {
        server_addr
    };
    if ipv6_candidate.parse::<Ipv6Addr>().is_ok() {
        return Ok(());
    }

    // 4. Check if it looks like an IPv4 address but failed parsing (e.g., "256.256.256.256")
    // This prevents "looking like IP but being treated as hostname" confusion
    let labels: Vec<&str> = server_addr.split('.').collect();
    if labels.len() >= 2
        && labels
            .iter()
            .all(|label| !label.is_empty() && label.chars().all(|c| c.is_ascii_digit()))
    {
        return Err(VmError::Validation(format!(
            "Invalid IP address format: {}",
            server_addr
        )));
    }

    // 5. Validate as hostname (DNS name)
    validate_hostname(server_addr)
}

/// Validate a hostname according to RFC 1123 rules
pub fn validate_hostname(hostname: &str) -> Result<()> {
    // Basic length check (DNS hostname max is 253 characters)
    if hostname.is_empty() || hostname.len() > 253 {
        return Err(VmError::Validation(
            "Hostname must be between 1 and 253 characters".to_string(),
        ));
    }

    // Cannot start or end with a dot
    if hostname.starts_with('.') || hostname.ends_with('.') {
        return Err(VmError::Validation(
            "Hostname cannot start or end with a dot".to_string(),
        ));
    }

    // Split into labels and validate each
    let labels: Vec<&str> = hostname.split('.').collect();
    if labels.is_empty() {
        return Err(VmError::Validation("Hostname cannot be empty".to_string()));
    }

    for label in labels {
        // Each label must be 1-63 characters
        if label.is_empty() || label.len() > 63 {
            return Err(VmError::Validation(
                "Hostname labels must be between 1 and 63 characters".to_string(),
            ));
        }

        // Cannot start or end with hyphen
        if label.starts_with('-') || label.ends_with('-') {
            return Err(VmError::Validation(
                "Hostname labels cannot start or end with a hyphen".to_string(),
            ));
        }

        // Must contain only alphanumeric characters and hyphens
        if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(VmError::Validation(format!(
                "Hostname label '{}' contains invalid characters (only alphanumeric and '-' allowed)",
                label
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_server_address_valid_ipv4() {
        assert!(validate_server_address("127.0.0.1").is_ok());
        assert!(validate_server_address("192.168.1.1").is_ok());
        assert!(validate_server_address("8.8.8.8").is_ok());
    }

    #[test]
    fn test_validate_server_address_valid_ipv6() {
        assert!(validate_server_address("::1").is_ok());
        assert!(validate_server_address("2001:db8::1").is_ok());
        assert!(validate_server_address("[::1]").is_ok());
    }

    #[test]
    fn test_validate_server_address_valid_hostnames() {
        assert!(validate_server_address("localhost").is_ok());
        assert!(validate_server_address("example.com").is_ok());
        assert!(validate_server_address("sub.example.com").is_ok());
        assert!(validate_server_address("my-server.local").is_ok());
    }

    #[test]
    fn test_validate_server_address_invalid() {
        // Invalid IPs
        assert!(validate_server_address("256.256.256.256").is_err());
        assert!(validate_server_address("192.168.1").is_err());

        // Injection attempts
        assert!(validate_server_address("127.0.0.1; rm -rf /").is_err());
        assert!(validate_server_address("host\nmalicious").is_err());

        // Invalid hostnames
        assert!(validate_server_address("-example.com").is_err());
        assert!(validate_server_address("example-.com").is_err());
        assert!(validate_server_address("exam ple.com").is_err());
    }
}
