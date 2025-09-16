use super::fixtures::ProjectTestFixture;
use crate::{detect_project_type, format_detected_types};

#[test]
fn test_multi_tech_detection() {
    let fixture = ProjectTestFixture::new().unwrap();

    // Create both React and Django indicators
    fixture
        .create_file(
            "package.json",
            r#"
    {
      "name": "fullstack-app",
      "dependencies": {
        "react": "^18.2.0"
      }
    }
    "#,
        )
        .unwrap();

    fixture
        .create_file("requirements.txt", "Django==5.1.3")
        .unwrap();

    let detected = detect_project_type(fixture.path());
    assert!(detected.contains("react"));
    assert!(detected.contains("django"));

    let formatted = format_detected_types(detected);
    assert!(formatted.starts_with("multi:"));
    assert!(formatted.contains("django"));
    assert!(formatted.contains("react"));
}

#[test]
fn test_multi_tech_with_docker() {
    let fixture = ProjectTestFixture::new().unwrap();

    fixture
        .create_file(
            "package.json",
            r#"
    {
      "name": "dockerized-react-app",
      "dependencies": {
        "react": "^18.2.0"
      }
    }
    "#,
        )
        .unwrap();

    fixture
        .create_file("Dockerfile", "FROM node:18-alpine")
        .unwrap();

    let detected = detect_project_type(fixture.path());
    let formatted = format_detected_types(detected);

    assert!(formatted.starts_with("multi:"));
    assert!(formatted.contains("docker"));
    assert!(formatted.contains("react"));
}