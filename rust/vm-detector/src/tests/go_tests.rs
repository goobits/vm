use super::fixtures::ProjectTestFixture;
use crate::{detect_project_type};

#[test]
fn test_go_detection() {
    let fixture = ProjectTestFixture::new().unwrap();
    fixture
        .create_file(
            "go.mod",
            r#"
    module test-go-app

    go 1.20

    require (
        github.com/gin-gonic/gin v1.9.1
    )
    "#,
        )
        .unwrap();

    let detected = detect_project_type(fixture.path());
    assert!(detected.contains("go"));
}