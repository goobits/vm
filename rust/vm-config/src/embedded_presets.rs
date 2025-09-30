use std::collections::HashMap;

/// Embedded preset files
pub fn get_embedded_presets() -> HashMap<&'static str, &'static str> {
    let mut presets = HashMap::new();

    presets.insert("base", include_str!("../../../configs/presets/base.yaml"));
    presets.insert(
        "tart-linux",
        include_str!("../../../configs/presets/tart-linux.yaml"),
    );
    presets.insert(
        "tart-macos",
        include_str!("../../../configs/presets/tart-macos.yaml"),
    );
    presets.insert(
        "tart-ubuntu",
        include_str!("../../../configs/presets/tart-ubuntu.yaml"),
    );

    presets
}

/// Get list of available preset names
pub fn get_preset_names() -> Vec<&'static str> {
    get_embedded_presets().keys().copied().collect()
}

/// Get preset content by name
pub fn get_preset_content(name: &str) -> Option<&'static str> {
    get_embedded_presets().get(name).copied()
}
