use super::fixtures::ProjectTestFixture;
use crate::detect_project_type;

#[test]
fn test_rails_detection() {
    let fixture = ProjectTestFixture::new().unwrap();
    fixture
        .create_file(
            "Gemfile",
            r#"
    source 'https://rubygems.org'
    gem 'rails', '~> 7.0.0'
    gem 'pg', '~> 1.1'
    "#,
        )
        .unwrap();

    let detected = detect_project_type(fixture.path());
    assert!(detected.contains("rails"));
    assert!(!detected.contains("ruby"));
}
