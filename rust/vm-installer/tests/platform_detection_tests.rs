use vm_installer::platform;

#[test]
#[cfg(feature = "integration")]
fn test_detect_platform_string_integration() {
    let platform = platform::detect_platform_string();
    assert!(!platform.is_empty(), "Platform string should not be empty");

    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    assert!(
        platform.contains(os),
        "Platform string '{}' should contain OS '{}'",
        platform,
        os
    );
    assert!(
        platform.contains(arch),
        "Platform string '{}' should contain architecture '{}'",
        platform,
        arch
    );
}
