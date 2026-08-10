use base64::Engine;

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

#[cfg(test)]
mod tests {
    use super::authorization_token;

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
}
