use std::path::PathBuf;
use tempfile::TempDir;
use vm_config::preset::PresetDetector;
use vm_config::preset_cache::{
    clear_preset_cache, get_cache_stats, list_presets_cached, load_preset_cached,
};

fn setup_detector() -> (TempDir, PresetDetector) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project_dir = temp_dir.path().to_path_buf();
    let presets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/presets");

    let detector = PresetDetector::new(project_dir, presets_dir);
    (temp_dir, detector)
}

#[test]
fn test_preset_caching_basic() {
    clear_preset_cache();

    let (_temp, detector) = setup_detector();

    // First load - should miss cache
    let initial_stats = get_cache_stats();
    assert_eq!(initial_stats.cached_presets, 0);

    let config1 = load_preset_cached(&detector, "base").expect("Failed to load base preset");

    // After first load, should be cached
    let after_first_stats = get_cache_stats();
    assert_eq!(after_first_stats.cached_presets, 1);

    // Second load - should hit cache (same result)
    let config2 = load_preset_cached(&detector, "base").expect("Failed to load base preset");

    // Should still only have 1 cached preset
    let after_second_stats = get_cache_stats();
    assert_eq!(after_second_stats.cached_presets, 1);

    // Configs should be identical
    assert_eq!(
        serde_json::to_string(&config1).unwrap(),
        serde_json::to_string(&config2).unwrap()
    );
}

#[test]
fn test_list_presets_caching() {
    clear_preset_cache();

    let (_temp, detector) = setup_detector();

    // First list - should miss cache
    let initial_stats = get_cache_stats();
    assert!(!initial_stats.list_cached);

    let list1 = list_presets_cached(&detector).expect("Failed to list presets");

    // After first list, should be cached
    let after_first_stats = get_cache_stats();
    assert!(after_first_stats.list_cached);

    // Second list - should hit cache
    let list2 = list_presets_cached(&detector).expect("Failed to list presets");

    // Lists should be identical
    assert_eq!(list1, list2);
    assert!(!list1.is_empty(), "Should have at least some presets");
}

#[test]
fn test_cache_invalidation() {
    clear_preset_cache();

    let (_temp, detector) = setup_detector();

    // Load a preset
    let _config1 = load_preset_cached(&detector, "base").expect("Failed to load base preset");
    let stats = get_cache_stats();
    assert_eq!(stats.cached_presets, 1);

    // Clear cache
    clear_preset_cache();

    // Verify cache is cleared
    let stats_after_clear = get_cache_stats();
    assert_eq!(stats_after_clear.cached_presets, 0);
    assert!(!stats_after_clear.list_cached);

    // Load again - should work
    let _config2 = load_preset_cached(&detector, "base").expect("Failed to load base preset");
    let final_stats = get_cache_stats();
    assert_eq!(final_stats.cached_presets, 1);
}

#[test]
fn test_multiple_presets_cached() {
    clear_preset_cache();

    let (_temp, detector) = setup_detector();

    // Load multiple presets
    let _base = load_preset_cached(&detector, "base").expect("Failed to load base");
    let stats1 = get_cache_stats();
    assert_eq!(stats1.cached_presets, 1);

    // Try to load another preset (may or may not exist depending on test environment)
    // We'll just verify the cache continues to work
    let list = list_presets_cached(&detector).expect("Failed to list presets");

    // If there are multiple presets, load another one
    if list.len() > 1 {
        let second_preset = list.iter().find(|p| *p != "base").unwrap();
        let _second =
            load_preset_cached(&detector, second_preset).expect("Failed to load second preset");

        let stats2 = get_cache_stats();
        assert_eq!(stats2.cached_presets, 2, "Should have 2 cached presets");
    }
}

#[test]
fn test_cache_clear_functionality() {
    clear_preset_cache();
    let stats = get_cache_stats();
    assert_eq!(stats.cached_presets, 0);
    assert!(!stats.list_cached);
}

#[test]
fn test_cache_with_detector_methods() {
    clear_preset_cache();

    let (_temp, detector) = setup_detector();

    // Use the detector's cached methods
    let config1 = detector
        .load_preset_cached("base")
        .expect("Failed to load base");
    let stats1 = get_cache_stats();
    assert_eq!(stats1.cached_presets, 1);

    // Load again via detector method
    let config2 = detector
        .load_preset_cached("base")
        .expect("Failed to load base");
    let stats2 = get_cache_stats();
    assert_eq!(stats2.cached_presets, 1);

    // Verify same result
    assert_eq!(
        serde_json::to_string(&config1).unwrap(),
        serde_json::to_string(&config2).unwrap()
    );

    // Test list_presets_cached
    let list1 = detector.list_presets_cached().expect("Failed to list");
    assert!(stats2.list_cached || get_cache_stats().list_cached);

    let list2 = detector.list_presets_cached().expect("Failed to list");
    assert_eq!(list1, list2);
}
