use base64::Engine;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use crate::{validate_label, PackageValidationError};

const AGENT_CAPABILITY_VERSION: &str = "v1";

/// Parse the authorization forms used by npm, Cargo, pip, and VM clients.
pub fn authorization_token(value: &str) -> Option<String> {
    let authorization = value.trim();
    if let Some(token) = authorization.strip_prefix("Bearer ") {
        return nonempty(token);
    }
    if let Some(encoded) = authorization.strip_prefix("Basic ") {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .ok()?;
        let credentials = String::from_utf8(decoded).ok()?;
        return credentials
            .split_once(':')
            .and_then(|(_, password)| nonempty(password));
    }
    nonempty(authorization)
}

fn nonempty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

/// Create a consumer-bound capability for package work performed by a guest.
pub fn issue_agent_capability(
    signing_key: &str,
    consumer: &str,
) -> Result<String, PackageValidationError> {
    validate_signing_key(signing_key)?;
    validate_label("consumer", consumer)?;
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(consumer);
    let signature = sign(signing_key, &payload)?;
    Ok(format!("{AGENT_CAPABILITY_VERSION}.{payload}.{signature}"))
}

/// Verify a guest capability and return its bound consumer identity.
pub fn verify_agent_capability(
    signing_key: &str,
    capability: &str,
) -> Result<String, PackageValidationError> {
    validate_signing_key(signing_key)?;
    let mut parts = capability.split('.');
    let version = parts.next();
    let payload = parts.next();
    let signature = parts.next();
    if version != Some(AGENT_CAPABILITY_VERSION)
        || payload.is_none()
        || signature.is_none()
        || parts.next().is_some()
    {
        return Err(PackageValidationError::new(
            "invalid package agent capability",
        ));
    }
    let payload = payload.expect("capability payload checked");
    let signature = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(signature.expect("capability signature checked"))
        .map_err(|_| PackageValidationError::new("invalid package agent capability"))?;
    let mut mac = Hmac::<Sha256>::new_from_slice(signing_key.as_bytes())
        .map_err(|_| PackageValidationError::new("invalid package agent signing key"))?;
    mac.update(payload.as_bytes());
    mac.verify_slice(&signature)
        .map_err(|_| PackageValidationError::new("invalid package agent capability"))?;
    let consumer = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()
        .and_then(|value| String::from_utf8(value).ok())
        .ok_or_else(|| PackageValidationError::new("invalid package agent capability"))?;
    validate_label("consumer", &consumer)?;
    Ok(consumer)
}

fn sign(signing_key: &str, payload: &str) -> Result<String, PackageValidationError> {
    let mut mac = Hmac::<Sha256>::new_from_slice(signing_key.as_bytes())
        .map_err(|_| PackageValidationError::new("invalid package agent signing key"))?;
    mac.update(payload.as_bytes());
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
}

fn validate_signing_key(signing_key: &str) -> Result<(), PackageValidationError> {
    if signing_key.len() < 32 {
        Err(PackageValidationError::new(
            "package agent signing key must contain at least 32 characters",
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{authorization_token, issue_agent_capability, verify_agent_capability};

    #[test]
    fn parses_package_manager_authorization_forms() {
        assert_eq!(
            authorization_token("Bearer read-token").as_deref(),
            Some("read-token")
        );
        assert_eq!(
            authorization_token("Basic cmVhZGVyOnJlYWQtdG9rZW4=").as_deref(),
            Some("read-token")
        );
        assert_eq!(
            authorization_token("publish-token").as_deref(),
            Some("publish-token")
        );
        assert_eq!(authorization_token("  "), None);
    }

    #[test]
    fn agent_capabilities_are_consumer_bound_and_tamper_evident() {
        let key = "signing-key-012345678901234567890123456789";
        let capability = issue_agent_capability(key, "project-a").unwrap();
        assert_eq!(
            verify_agent_capability(key, &capability).unwrap(),
            "project-a"
        );
        assert!(verify_agent_capability(key, &capability.replace("v1.", "v2.")).is_err());
        assert!(
            verify_agent_capability("different-key-012345678901234567890123456", &capability)
                .is_err()
        );
    }
}
