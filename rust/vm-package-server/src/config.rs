/// Runtime authentication policy for the package registry.
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub security: SecurityConfig,
}

/// Separate read and publish credentials keep edge caches read-only.
#[derive(Debug, Clone, Default)]
pub struct SecurityConfig {
    pub require_authentication: bool,
    pub read_keys: Vec<String>,
    pub publish_keys: Vec<String>,
}
