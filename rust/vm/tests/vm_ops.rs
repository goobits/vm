#[path = "vm_ops"]
mod vm_ops {
    // Test modules
    pub mod create_destroy_tests;
    pub mod feature_tests;
    pub mod interaction_tests;
    pub mod lifecycle_integration_tests;
    pub mod multi_instance_tests;
    pub mod provider_parity_tests;
    pub mod service_lifecycle_tests;
    pub mod status_tests;

    // Re-export helpers for easy access in test modules
    pub mod helpers;
}
