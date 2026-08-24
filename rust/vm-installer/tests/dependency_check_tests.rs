#[cfg(feature = "integration")]
use vm_installer::check_dependencies;

#[test]
#[cfg(feature = "integration")]
fn test_dependency_check_success() {
    // In the test environment, we expect Rust and Cargo to be installed.
    // This test verifies that the check() function succeeds in this case.
    let result = check_dependencies();
    assert!(
        result.is_ok(),
        "Dependency check should pass in a valid Rust environment"
    );
}
