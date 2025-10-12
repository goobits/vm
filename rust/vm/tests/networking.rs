// Entrypoint for networking-related integration tests.

#[path = "networking"]
mod networking {
    pub mod port_forwarding;
    pub mod ssh_refresh;
}
