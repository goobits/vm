use super::fixtures::ProjectTestFixture;
use crate::{detect_project_type};

#[test]
fn test_docker_detection() {
    let fixture = ProjectTestFixture::new().unwrap();
    fixture
        .create_file("Dockerfile", "FROM node:18-alpine\nWORKDIR /app")
        .unwrap();

    let detected = detect_project_type(fixture.path());
    assert!(detected.contains("docker"));
}

#[test]
fn test_docker_compose_detection() {
    let fixture = ProjectTestFixture::new().unwrap();
    fixture
        .create_file(
            "docker-compose.yml",
            r#"
    version: '3.8'
    services:
      app:
        build: .
    "#,
        )
        .unwrap();

    let detected = detect_project_type(fixture.path());
    assert!(detected.contains("docker"));
}