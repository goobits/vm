use tracing_subscriber::fmt::format::FmtSpan;

fn main() {
    // Initialize tracing subscriber with detailed settings
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_span_events(FmtSpan::ENTER | FmtSpan::CLOSE)
        .with_target(true)
        .init();

    // Simulate preset loading
    let presets_dir = std::path::PathBuf::from("/tmp");
    let project_dir = std::env::current_dir().unwrap();

    let detector = vm_config::preset::PresetDetector::new(project_dir, presets_dir);

    println!("\n=== Testing list_all_presets ===");
    if let Ok(presets) = detector.list_all_presets() {
        println!("Found {} presets", presets.len());
    }

    println!("\n=== Testing load_preset ===");
    if let Ok(_config) = detector.load_preset("base") {
        println!("Successfully loaded 'base' preset");
    }

    println!("\n=== Testing get_preset_description ===");
    if let Some(desc) = detector.get_preset_description("base") {
        println!("Description: {}", desc);
    }

    println!("\n=== Testing discover_plugins ===");
    if let Ok(plugins) = vm_plugin::discover_plugins() {
        println!("Discovered {} plugins", plugins.len());
    }
}
