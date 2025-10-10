use serde::{Deserialize, Serialize};

/// Defines a single port forwarding rule.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PortMapping {
    /// The port on the host machine.
    pub host: u16,
    /// The port inside the guest VM.
    pub guest: u16,
    /// The network protocol (TCP or UDP). Defaults to TCP.
    #[serde(default)]
    pub protocol: Protocol,
}

/// Represents the network protocol for port mapping.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Tcp,
    Udp,
}

/// The default protocol for port mappings is TCP.
impl Default for Protocol {
    fn default() -> Self {
        Self::Tcp
    }
}
