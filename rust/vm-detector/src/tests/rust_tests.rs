use super::fixtures::ProjectTestFixture;
use crate::detect_project_type;

#[test]
fn test_rust_detection() {
    let fixture = ProjectTestFixture::new().unwrap();
    fixture
        .create_file(
            "Cargo.toml",
            r#"
    [package]
    name = "test-rust-app"
    version = "0.1.0"
    edition = "2021"

    [dependencies]
    tokio = "1.0"
    "#,
        )
        .unwrap();

    let detected = detect_project_type(fixture.path());
    assert!(detected.contains("rust"));
}
