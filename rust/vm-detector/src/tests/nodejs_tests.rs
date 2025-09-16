use super::fixtures::ProjectTestFixture;
use crate::{detect_project_type, format_detected_types};

#[test]
fn test_react_detection() {
    let fixture = ProjectTestFixture::new().unwrap();
    fixture
        .create_file(
            "package.json",
            r#"
    {
      "name": "test-react-app",
      "version": "1.0.0",
      "dependencies": {
        "react": "^18.3.1",
        "react-dom": "^18.3.1"
      }
    }
    "#,
        )
        .unwrap();

    let detected = detect_project_type(fixture.path());
    assert!(detected.contains("react"));
    assert!(!detected.contains("nodejs"));

    let formatted = format_detected_types(detected);
    assert_eq!(formatted, "react");
}

#[test]
fn test_vue_detection() {
    let fixture = ProjectTestFixture::new().unwrap();
    fixture
        .create_file(
            "package.json",
            r#"
    {
      "name": "test-vue-app",
      "dependencies": {
        "vue": "^3.3.0"
      }
    }
    "#,
        )
        .unwrap();

    let detected = detect_project_type(fixture.path());
    assert!(detected.contains("vue"));

    let formatted = format_detected_types(detected);
    assert_eq!(formatted, "vue");
}

#[test]
fn test_next_detection_overrides_react() {
    let fixture = ProjectTestFixture::new().unwrap();
    fixture
        .create_file(
            "package.json",
            r#"
    {
      "name": "test-nextjs-app",
      "dependencies": {
        "next": "^13.4.0",
        "react": "^18.3.1",
        "react-dom": "^18.3.1"
      }
    }
    "#,
        )
        .unwrap();

    let detected = detect_project_type(fixture.path());
    assert!(detected.contains("next"));
    assert!(!detected.contains("react"));
}

#[test]
fn test_angular_detection() {
    let fixture = ProjectTestFixture::new().unwrap();
    fixture
        .create_file(
            "package.json",
            r#"
    {
      "name": "test-angular-app",
      "dependencies": {
        "@angular/core": "^15.2.0",
        "@angular/common": "^15.2.0"
      }
    }
    "#,
        )
        .unwrap();

    let detected = detect_project_type(fixture.path());
    assert!(detected.contains("angular"));
}

#[test]
fn test_nodejs_detection() {
    let fixture = ProjectTestFixture::new().unwrap();
    fixture
        .create_file(
            "package.json",
            r#"
    {
      "name": "test-nodejs-app",
      "dependencies": {
        "express": "^4.21.1",
        "lodash": "^4.17.21"
      }
    }
    "#,
        )
        .unwrap();

    let detected = detect_project_type(fixture.path());
    assert!(detected.contains("nodejs"));
}

#[test]
fn test_framework_priority_ordering() {
    // Test that specific frameworks take priority over generic ones
    let fixture = ProjectTestFixture::new().unwrap();

    // Create package.json with Next.js (should override React detection)
    fixture
        .create_file(
            "package.json",
            r#"
    {
      "name": "test-app",
      "dependencies": {
        "react": "^18.3.1",
        "react-dom": "^18.3.1",
        "next": "^13.4.0"
      }
    }
    "#,
        )
        .unwrap();

    let detected = detect_project_type(fixture.path());

    // Should detect Next.js, not React (Next.js wins due to break statement)
    assert!(detected.contains("next"));
    assert!(!detected.contains("react"));
    assert!(!detected.contains("nodejs"));
}
