// Entrypoint for shared services integration tests.

#[cfg(feature = "integration")]
#[path = "services"]
mod services {
    pub mod shared_services;
}
