use super::fixtures::ProjectTestFixture;
use crate::detector::{detect_project_type, format_detected_types};
use std::path::Path;

#[test]
fn test_empty_directory() {
    let fixture = ProjectTestFixture::new().unwrap();

    let detected = detect_project_type(fixture.path());
    let formatted = format_detected_types(detected);

    assert_eq!(formatted, "generic");
}

#[test]
fn test_malformed_json_graceful_handling() {
    let fixture = ProjectTestFixture::new().unwrap();

    // Create malformed JSON
    fixture
        .create_file(
            "package.json",
            r#"
    {
      "name": "broken-app",
      "version": "1.0.0"
      "dependencies": {
        "react": "^18.2.0"
      // Missing closing braces
    "#,
        )
        .unwrap();

    let detected = detect_project_type(fixture.path());
    let formatted = format_detected_types(detected);

    // Should gracefully fall back to generic when JSON is malformed
    assert_eq!(formatted, "generic");
}

#[test]
fn test_missing_files_handling() {
    let _fixture = ProjectTestFixture::new().unwrap();

    // Test with various non-existent files
    let detected = detect_project_type(Path::new("/nonexistent/path"));
    let formatted = format_detected_types(detected);

    assert_eq!(formatted, "generic");
}
