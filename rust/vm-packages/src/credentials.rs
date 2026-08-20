use base64::Engine;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::{normalize_remote_repository_url, validate_label, PackageValidationError};

const AGENT_CAPABILITY_V1: &str = "v1";
const AGENT_CAPABILITY_V2: &str = "v2";

/// HTTP header used alongside a checkout lease when an operation must retain
/// the guest's signed package-agent authority.
pub const AGENT_CAPABILITY_HEADER: &str = "x-vm-agent-capability";

/// Authenticated guest identity. Repository binding is present only for an
/// explicitly registered canonical workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCapabilityClaims {
    pub consumer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_repository: Option<String>,
}

impl AgentCapabilityClaims {
    pub fn new(
        consumer: impl Into<String>,
        canonical_repository: Option<String>,
    ) -> Result<Self, PackageValidationError> {
        let consumer = consumer.into();
        validate_label("consumer", &consumer)?;
        let canonical_repository = canonical_repository
            .map(|repository| normalize_remote_repository_url(&repository))
            .transpose()?;
        Ok(Self {
            consumer,
            canonical_repository,
        })
    }
}

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
    Ok(format!("{AGENT_CAPABILITY_V1}.{payload}.{signature}"))
}

/// Create a structured capability for an explicitly attested workspace.
pub fn issue_agent_capability_v2(
    signing_key: &str,
    claims: &AgentCapabilityClaims,
) -> Result<String, PackageValidationError> {
    validate_signing_key(signing_key)?;
    let claims =
        AgentCapabilityClaims::new(claims.consumer.clone(), claims.canonical_repository.clone())?;
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&claims)
            .map_err(|_| PackageValidationError::new("invalid package agent capability"))?,
    );
    let signed = format!("{AGENT_CAPABILITY_V2}.{payload}");
    let signature = sign(signing_key, &signed)?;
    Ok(format!("{signed}.{signature}"))
}

/// Verify a v1 or v2 guest capability and return its authenticated claims.
pub fn verify_agent_capability(
    signing_key: &str,
    capability: &str,
) -> Result<AgentCapabilityClaims, PackageValidationError> {
    validate_signing_key(signing_key)?;
    let mut parts = capability.split('.');
    let version = parts.next();
    let payload = parts.next();
    let signature = parts.next();
    if !matches!(version, Some(AGENT_CAPABILITY_V1 | AGENT_CAPABILITY_V2))
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
    let signed = if version == Some(AGENT_CAPABILITY_V2) {
        format!("{AGENT_CAPABILITY_V2}.{payload}")
    } else {
        payload.to_string()
    };
    verify_signature(signing_key, &signed, &signature)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| PackageValidationError::new("invalid package agent capability"))?;
    if version == Some(AGENT_CAPABILITY_V1) {
        let consumer = String::from_utf8(decoded)
            .map_err(|_| PackageValidationError::new("invalid package agent capability"))?;
        return AgentCapabilityClaims::new(consumer, None);
    }
    let claims: AgentCapabilityClaims = serde_json::from_slice(&decoded)
        .map_err(|_| PackageValidationError::new("invalid package agent capability"))?;
    AgentCapabilityClaims::new(claims.consumer, claims.canonical_repository)
}

fn verify_signature(
    signing_key: &str,
    payload: &str,
    signature: &[u8],
) -> Result<(), PackageValidationError> {
    let mut mac = Hmac::<Sha256>::new_from_slice(signing_key.as_bytes())
        .map_err(|_| PackageValidationError::new("invalid package agent signing key"))?;
    mac.update(payload.as_bytes());
    mac.verify_slice(signature)
        .map_err(|_| PackageValidationError::new("invalid package agent capability"))
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
    use super::{
        authorization_token, issue_agent_capability, issue_agent_capability_v2,
        verify_agent_capability, AgentCapabilityClaims,
    };

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
            AgentCapabilityClaims::new("project-a", None).unwrap()
        );
        assert!(verify_agent_capability(key, &capability.replace("v1.", "v2.")).is_err());
        assert!(
            verify_agent_capability("different-key-012345678901234567890123456", &capability)
                .is_err()
        );
    }

    #[test]
    fn v2_capabilities_bind_normalized_canonical_repositories() {
        let key = "signing-key-012345678901234567890123456789";
        let claims =
            AgentCapabilityClaims::new("project-a", Some("git@github.com:team/project.git".into()))
                .unwrap();
        let capability = issue_agent_capability_v2(key, &claims).unwrap();

        let verified = verify_agent_capability(key, &capability).unwrap();
        assert_eq!(verified.consumer, "project-a");
        assert!(crate::repository_urls_equivalent(
            verified.canonical_repository.as_deref().unwrap(),
            "https://github.com/team/project.git"
        ));
        let mut tampered = capability.clone().into_bytes();
        tampered[3] ^= 1;
        assert!(verify_agent_capability(key, &String::from_utf8(tampered).unwrap()).is_err());
        assert!(verify_agent_capability(key, &capability.replace("v2.", "v1.")).is_err());
    }
}
