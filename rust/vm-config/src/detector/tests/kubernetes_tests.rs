use super::fixtures::ProjectTestFixture;
use crate::detector::detect_project_type;

#[test]
fn test_kubernetes_detection() {
    let fixture = ProjectTestFixture::new().unwrap();
    fixture.create_dir("k8s").unwrap();
    fixture
        .create_file(
            "k8s/deployment.yaml",
            r#"
    apiVersion: apps/v1
    kind: Deployment
    metadata:
      name: test-app
    "#,
        )
        .unwrap();

    let detected = detect_project_type(fixture.path());
    assert!(detected.contains("kubernetes"));
}
