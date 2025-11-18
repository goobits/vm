use std::path::PathBuf;
use std::time::Instant;
use vm_config::preset::PresetDetector;
use vm_config::preset_cache::{clear_preset_cache, get_cache_stats};

fn main() {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_env_filter("vm_config=debug")
        .try_init();

    let project_dir = std::env::current_dir().unwrap();
    let presets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vm-plugin/presets");

    let detector = PresetDetector::new(project_dir, presets_dir);

    println!("=== Preset Caching Performance Benchmark ===\n");

    // Clear cache to start fresh
    clear_preset_cache();

    // Benchmark: Load preset without cache (first time)
    let start = Instant::now();
    let _config1 = detector.load_preset("base").expect("Failed to load preset");
    let uncached_duration = start.elapsed();
    println!("First load (uncached):  {:?}", uncached_duration);

    // Benchmark: Load preset with cache (second time)
    let start = Instant::now();
    let _config2 = detector
        .load_preset_cached("base")
        .expect("Failed to load preset");
    let cached_duration = start.elapsed();
    println!("Second load (cached):   {:?}", cached_duration);

    println!(
        "\nSpeedup: {:.2}x faster",
        uncached_duration.as_nanos() as f64 / cached_duration.as_nanos().max(1) as f64
    );

    // Benchmark: Multiple loads from cache
    let iterations = 100;
    clear_preset_cache();

    let start = Instant::now();
    for _ in 0..iterations {
        let _ = detector.load_preset("base").unwrap();
    }
    let uncached_total = start.elapsed();

    clear_preset_cache();
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = detector.load_preset_cached("base").unwrap();
    }
    let cached_total = start.elapsed();

    println!("\n=== Repeated Loads ({} iterations) ===", iterations);
    println!(
        "Without cache: {:?} ({:.2}µs/load)",
        uncached_total,
        uncached_total.as_micros() as f64 / iterations as f64
    );
    println!(
        "With cache:    {:?} ({:.2}µs/load)",
        cached_total,
        cached_total.as_micros() as f64 / iterations as f64
    );
    println!(
        "Speedup: {:.2}x faster",
        uncached_total.as_nanos() as f64 / cached_total.as_nanos().max(1) as f64
    );

    // Show cache statistics
    let stats = get_cache_stats();
    println!("\n=== Cache Statistics ===");
    println!("Cached presets: {}", stats.cached_presets);
    println!("List cached: {}", stats.list_cached);

    // Benchmark list_presets
    println!("\n=== List Presets Benchmark ===");
    clear_preset_cache();

    let start = Instant::now();
    let _list1 = detector.list_presets().expect("Failed to list presets");
    let uncached_list = start.elapsed();
    println!("First list (uncached): {:?}", uncached_list);

    let start = Instant::now();
    let _list2 = detector
        .list_presets_cached()
        .expect("Failed to list presets");
    let cached_list = start.elapsed();
    println!("Second list (cached):  {:?}", cached_list);

    println!(
        "\nSpeedup: {:.2}x faster",
        uncached_list.as_nanos() as f64 / cached_list.as_nanos().max(1) as f64
    );
}
