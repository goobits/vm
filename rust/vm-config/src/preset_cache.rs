//! Preset caching layer using once_cell for performance
//!
//! Caches preset metadata and content to avoid repeated file I/O
//! and plugin discovery operations.

use crate::config::VmConfig;
use crate::preset::PresetDetector;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::instrument;
use vm_core::error::Result;

/// Cached preset data
#[derive(Debug, Clone)]
struct CachedPreset {
    /// The parsed preset configuration
    config: VmConfig,
    /// Preset description (from plugin metadata)
    #[allow(dead_code)]
    description: Option<String>,
    /// Timestamp when cached (for potential TTL)
    #[allow(dead_code)]
    cached_at: std::time::Instant,
}

/// Global preset cache
///
/// Uses RwLock for concurrent read access with occasional write for cache updates.
/// Keyed by preset name.
static PRESET_CACHE: Lazy<Arc<RwLock<HashMap<String, CachedPreset>>>> =
    Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));

/// List of all available preset names (cached)
static PRESET_LIST_CACHE: Lazy<Arc<RwLock<Option<Vec<String>>>>> =
    Lazy::new(|| Arc::new(RwLock::new(None)));

/// Load a preset with caching
///
/// First checks the cache, then falls back to PresetDetector if not cached.
#[instrument(skip(detector), fields(name = %name))]
pub fn load_preset_cached(detector: &PresetDetector, name: &str) -> Result<VmConfig> {
    // Try to read from cache first
    {
        let cache = PRESET_CACHE.read().unwrap();
        if let Some(cached) = cache.get(name) {
            tracing::debug!("Preset '{}' found in cache", name);
            return Ok(cached.config.clone());
        }
    }

    // Cache miss - load from detector
    tracing::debug!("Preset '{}' not in cache, loading from filesystem", name);
    let config = detector.load_preset(name)?;
    let description = detector.get_preset_description(name);

    // Write to cache
    {
        let mut cache = PRESET_CACHE.write().unwrap();
        cache.insert(
            name.to_string(),
            CachedPreset {
                config: config.clone(),
                description,
                cached_at: std::time::Instant::now(),
            },
        );
    }

    Ok(config)
}

/// List all presets with caching
#[instrument(skip(detector))]
pub fn list_presets_cached(detector: &PresetDetector) -> Result<Vec<String>> {
    // Try cache first
    {
        let cache = PRESET_LIST_CACHE.read().unwrap();
        if let Some(ref list) = *cache {
            tracing::debug!("Preset list found in cache ({} presets)", list.len());
            return Ok(list.clone());
        }
    }

    // Cache miss - load from detector
    tracing::debug!("Preset list not in cache, scanning filesystem");
    let list = detector.list_presets()?;

    // Write to cache
    {
        let mut cache = PRESET_LIST_CACHE.write().unwrap();
        *cache = Some(list.clone());
    }

    Ok(list)
}

/// List all presets (including box presets) with caching
#[instrument(skip(detector))]
pub fn list_all_presets_cached(detector: &PresetDetector) -> Result<Vec<String>> {
    // For now, don't cache list_all_presets separately since it's only used in init
    // and init is less frequent than config operations
    detector.list_all_presets()
}

/// Clear the preset cache
///
/// Call this when preset files are modified or plugins are added/removed.
pub fn clear_preset_cache() {
    tracing::info!("Clearing preset cache");
    {
        let mut cache = PRESET_CACHE.write().unwrap();
        cache.clear();
    }
    {
        let mut cache = PRESET_LIST_CACHE.write().unwrap();
        *cache = None;
    }
}

/// Get cache statistics (for debugging/monitoring)
pub fn get_cache_stats() -> CacheStats {
    let preset_cache = PRESET_CACHE.read().unwrap();
    let list_cache = PRESET_LIST_CACHE.read().unwrap();

    CacheStats {
        cached_presets: preset_cache.len(),
        list_cached: list_cache.is_some(),
    }
}

#[derive(Debug)]
pub struct CacheStats {
    pub cached_presets: usize,
    pub list_cached: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_clear() {
        clear_preset_cache();
        let stats = get_cache_stats();
        assert_eq!(stats.cached_presets, 0);
        assert!(!stats.list_cached);
    }
}
