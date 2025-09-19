//! Network-related error handling

use crate::{vm_error, vm_error_hint};

/// Handle port already in use error
pub fn port_in_use(port: u16) -> anyhow::Error {
    vm_error!("Port {} is already in use", port);
    vm_error_hint!("Find what's using it: lsof -i :{}", port);
    anyhow::anyhow!("Port {} unavailable", port)
}

/// Handle network connectivity error
pub fn network_unreachable(target: &str) -> anyhow::Error {
    vm_error!("Cannot reach {}", target);
    vm_error_hint!("Check your internet connection and firewall settings");
    anyhow::anyhow!("Network unreachable: {}", target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_port_in_use() {
        let err = port_in_use(8080);
        assert!(err.to_string().contains("Port 8080 unavailable"));
    }
}
