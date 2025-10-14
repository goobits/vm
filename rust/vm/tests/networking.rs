// Entrypoint for networking-related integration tests.
//
// NOTE: Most tests in this module are marked with #[ignore] because they
// create real Docker containers and can be slow or cause hangs if Docker
// has issues.
//
// To run all tests including ignored ones:
//   cargo test --test networking --features integration -- --ignored
//
// To run only non-ignored tests (default):
//   cargo test --test networking --features integration

#[cfg(feature = "integration")]
#[path = "networking"]
mod networking {
    pub mod port_forwarding;
    pub mod ssh_refresh;
}
