use super::fixtures::ProjectTestFixture;
use crate::detector::detect_project_type;

#[test]
fn test_django_detection() {
    let fixture = ProjectTestFixture::new().unwrap();
    fixture
        .create_file("requirements.txt", "Django==5.1.3\npsycopg2-binary==2.9.9")
        .unwrap();

    let detected = detect_project_type(fixture.path());
    assert!(detected.contains("django"));
    assert!(!detected.contains("python"));
}

#[test]
fn test_flask_detection() {
    let fixture = ProjectTestFixture::new().unwrap();
    fixture
        .create_file("requirements.txt", "Flask==3.1.0\nFlask-SQLAlchemy==3.1.1")
        .unwrap();

    let detected = detect_project_type(fixture.path());
    assert!(detected.contains("flask"));
}

#[test]
fn test_python_detection() {
    let fixture = ProjectTestFixture::new().unwrap();
    fixture
        .create_file("requirements.txt", "requests==2.31.0\nnumpy==1.24.0")
        .unwrap();

    let detected = detect_project_type(fixture.path());
    assert!(detected.contains("python"));
}

#[test]
fn test_pyproject_toml_detection() {
    let fixture = ProjectTestFixture::new().unwrap();
    fixture
        .create_file(
            "pyproject.toml",
            r#"
    [tool.poetry]
    name = "test-python-app"
    version = "0.1.0"

    [tool.poetry.dependencies]
    python = "^3.9"
    "#,
        )
        .unwrap();

    let detected = detect_project_type(fixture.path());
    assert!(detected.contains("python"));
}
