#![cfg(all(test, target_os = "macos"))]

use std::path::Path;
use uuid::Uuid;
use vm_config::config::{ProjectConfig, VmConfig};
use vm_core::error::Result;
use vm_provider::{tart::provider::TartProvider, Provider};

struct TestFixture {
    vm_name: String,
    provider: TartProvider,
}

impl TestFixture {
    fn new() -> Result<Self> {
        let vm_name = format!("vm-test-{}", Uuid::new_v4());
        let config = VmConfig {
            provider: Some("tart".to_string()),
            project: Some(ProjectConfig {
                name: Some(vm_name.clone()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let provider = TartProvider::new(config)?;
        Ok(Self { vm_name, provider })
    }
}

impl Drop for TestFixture {
    fn drop(&mut self) {
        // Ensure the VM is destroyed after the test
        let _ = self.provider.destroy(None);
    }
}

#[test]
#[ignore] // This is an integration test that requires Tart to be installed
fn test_tart_ssh_path_integration() -> Result<()> {
    // Setup
    let fixture = TestFixture::new()?;
    fixture.provider.create(None)?;

    let workspace_path = fixture.provider.get_sync_directory();
    let test_dir_name = "test_dir";
    let test_dir_path = Path::new(&workspace_path).join(test_dir_name);

    // Create a directory inside the VM for testing
    let mkdir_cmd = vec![
        "mkdir".to_string(),
        "-p".to_string(),
        test_dir_path.to_str().unwrap().to_string(),
    ];
    fixture.provider.exec(None, &mkdir_cmd)?;

    // Execute `pwd` in the new directory
    let output = fixture
        .provider
        .exec_in_path(None, &test_dir_path, &["pwd"])?;

    // Verify
    assert_eq!(output.trim(), test_dir_path.to_str().unwrap());

    Ok(())
}
