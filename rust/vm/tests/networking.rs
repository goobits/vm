// Entrypoint for networking-related integration tests.

#[cfg(feature = "integration")]
#[path = "networking"]
mod networking {
    pub mod port_forwarding;
    pub mod ssh_refresh;
}
