use vm_docker_registry::auto_manager::AutoManager;
use vm_docker_registry::types::AutoConfig;

#[test]
#[cfg(feature = "integration")]
fn test_auto_manager_creation_integration() {
    let manager = AutoManager::new();
    let config = AutoConfig::default();

    let status = tokio_test::block_on(manager.get_status()).unwrap();

    assert_eq!(status.config.max_cache_size_gb, config.max_cache_size_gb);
    assert_eq!(status.config.max_image_age_days, config.max_image_age_days);
    assert_eq!(status.restart_attempts, 0);
    assert!(status.last_cleanup.is_none());
}
