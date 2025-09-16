use super::fixtures::ProjectTestFixture;
use crate::detect_project_type;

#[test]
fn test_php_detection() {
    let fixture = ProjectTestFixture::new().unwrap();
    fixture
        .create_file(
            "composer.json",
            r#"
    {
      "name": "test/php-app",
      "require": {
        "php": ">=8.0"
      }
    }
    "#,
        )
        .unwrap();

    let detected = detect_project_type(fixture.path());
    assert!(detected.contains("php"));
}
