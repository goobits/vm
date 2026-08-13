use serde::{Deserialize, Serialize};

use super::PortMapping;

/// Port configuration with range-based allocation and explicit mappings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PortsConfig {
    #[serde(rename = "_range", skip_serializing_if = "Option::is_none")]
    pub range: Option<Vec<u16>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mappings: Vec<PortMapping>,
}

impl PortsConfig {
    pub fn get_all_exposed_ports(&self) -> Vec<String> {
        let mut ports = self
            .mappings
            .iter()
            .map(|mapping| format!("{}:{}", mapping.host, mapping.guest))
            .collect::<Vec<_>>();
        if let Some(range) = &self.range {
            if range.len() == 2 {
                ports.push(format!(
                    "{}-{}:{}-{}",
                    range[0], range[1], range[0], range[1]
                ));
            }
        }
        ports
    }

    pub fn has_ports(&self) -> bool {
        self.range.is_some() || !self.mappings.is_empty()
    }

    pub fn is_port_in_range(&self, port: u16) -> bool {
        self.range
            .as_ref()
            .filter(|range| range.len() == 2)
            .is_some_and(|range| port >= range[0] && port <= range[1])
    }
}
