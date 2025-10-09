// Test modules
#[path = "vm_ops/create_destroy_tests.rs"]
pub mod create_destroy_tests;
#[path = "vm_ops/multi_instance_tests.rs"]
pub mod multi_instance_tests;
#[path = "vm_ops/provider_parity_tests.rs"]
pub mod provider_parity_tests;
#[path = "vm_ops/service_lifecycle_tests.rs"]
pub mod service_lifecycle_tests;

// Re-export helpers for easy access in test modules
#[path = "vm_ops/helpers.rs"]
pub mod helpers;