use vm_config::config::BoxSpec;
use vm_provider::BoxConfig;

// =============================================================================
// Vagrant Provider Tests
// =============================================================================

#[test]
fn test_vagrant_valid_box() {
    let spec = BoxSpec::String("ubuntu/focal64".to_string());
    let result = BoxConfig::parse_for_vagrant(&spec).unwrap();
    assert!(matches!(result, BoxConfig::VagrantBox(_)));
    if let BoxConfig::VagrantBox(name) = result {
        assert_eq!(name, "ubuntu/focal64");
    }
}

#[test]
fn test_vagrant_invalid_format_errors() {
    let spec = BoxSpec::String("ubuntu".to_string());
    let result = BoxConfig::parse_for_vagrant(&spec);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("user/boxname"));
}

#[test]
fn test_vagrant_rejects_dockerfile() {
    let spec = BoxSpec::Build {
        dockerfile: "./Dockerfile".to_string(),
        context: None,
        args: None,
    };
    let result = BoxConfig::parse_for_vagrant(&spec);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("does not support Dockerfile"));
}

#[test]
fn test_vagrant_handles_snapshot() {
    let spec = BoxSpec::String("@my-snapshot".to_string());
    let result = BoxConfig::parse_for_vagrant(&spec).unwrap();
    assert!(matches!(result, BoxConfig::Snapshot(_)));
    if let BoxConfig::Snapshot(name) = result {
        assert_eq!(name, "my-snapshot");
    }
}

#[test]
fn test_vagrant_valid_box_with_version() {
    // Vagrant boxes can have versions, but we treat the whole string as the box name
    let spec = BoxSpec::String("hashicorp/bionic64".to_string());
    let result = BoxConfig::parse_for_vagrant(&spec).unwrap();
    assert!(matches!(result, BoxConfig::VagrantBox(_)));
    if let BoxConfig::VagrantBox(name) = result {
        assert_eq!(name, "hashicorp/bionic64");
    }
}

#[test]
fn test_vagrant_empty_string_errors() {
    let spec = BoxSpec::String("".to_string());
    let result = BoxConfig::parse_for_vagrant(&spec);
    assert!(result.is_err());
}

#[test]
fn test_vagrant_only_slash_errors() {
    let spec = BoxSpec::String("/".to_string());
    let result = BoxConfig::parse_for_vagrant(&spec);
    // Stricter validation now rejects single "/" as invalid
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("non-empty user and box name"));
}

#[test]
fn test_vagrant_slash_at_start_errors() {
    let spec = BoxSpec::String("/boxname".to_string());
    let result = BoxConfig::parse_for_vagrant(&spec);
    // Empty user part should be rejected
    assert!(result.is_err());
}

#[test]
fn test_vagrant_slash_at_end_errors() {
    let spec = BoxSpec::String("user/".to_string());
    let result = BoxConfig::parse_for_vagrant(&spec);
    // Empty box name part should be rejected
    assert!(result.is_err());
}

#[test]
fn test_vagrant_multiple_slashes() {
    // Some boxes might have nested paths like "company/team/box"
    let spec = BoxSpec::String("company/team/box".to_string());
    let result = BoxConfig::parse_for_vagrant(&spec).unwrap();
    assert!(matches!(result, BoxConfig::VagrantBox(_)));
    if let BoxConfig::VagrantBox(name) = result {
        assert_eq!(name, "company/team/box");
    }
}

// =============================================================================
// Tart Provider Tests
// =============================================================================

#[test]
fn test_tart_parses_oci_image() {
    let spec = BoxSpec::String("ghcr.io/cirruslabs/ubuntu:latest".to_string());
    let result = BoxConfig::parse_for_tart(&spec).unwrap();
    assert!(matches!(result, BoxConfig::TartImage(_)));
    if let BoxConfig::TartImage(image) = result {
        assert_eq!(image, "ghcr.io/cirruslabs/ubuntu:latest");
    }
}

#[test]
fn test_tart_handles_snapshot() {
    let spec = BoxSpec::String("@my-snapshot".to_string());
    let result = BoxConfig::parse_for_tart(&spec).unwrap();
    assert!(matches!(result, BoxConfig::Snapshot(_)));
    if let BoxConfig::Snapshot(name) = result {
        assert_eq!(name, "my-snapshot");
    }
}

#[test]
fn test_tart_rejects_dockerfile() {
    let spec = BoxSpec::Build {
        dockerfile: "./Dockerfile".to_string(),
        context: None,
        args: None,
    };
    let result = BoxConfig::parse_for_tart(&spec);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("does not support Dockerfile"));
}

#[test]
fn test_tart_parses_simple_image() {
    // Simple image name without registry
    let spec = BoxSpec::String("ubuntu:latest".to_string());
    let result = BoxConfig::parse_for_tart(&spec).unwrap();
    assert!(matches!(result, BoxConfig::TartImage(_)));
    if let BoxConfig::TartImage(image) = result {
        assert_eq!(image, "ubuntu:latest");
    }
}

#[test]
fn test_tart_parses_image_without_tag() {
    let spec = BoxSpec::String("ghcr.io/cirruslabs/ubuntu".to_string());
    let result = BoxConfig::parse_for_tart(&spec).unwrap();
    assert!(matches!(result, BoxConfig::TartImage(_)));
    if let BoxConfig::TartImage(image) = result {
        assert_eq!(image, "ghcr.io/cirruslabs/ubuntu");
    }
}

#[test]
fn test_tart_parses_image_with_digest() {
    let spec = BoxSpec::String("ghcr.io/cirruslabs/ubuntu@sha256:abc123".to_string());
    let result = BoxConfig::parse_for_tart(&spec).unwrap();
    assert!(matches!(result, BoxConfig::TartImage(_)));
    if let BoxConfig::TartImage(image) = result {
        assert_eq!(image, "ghcr.io/cirruslabs/ubuntu@sha256:abc123");
    }
}

#[test]
fn test_tart_snapshot_with_complex_name() {
    let spec = BoxSpec::String("@snapshot-with-dashes_and_underscores.123".to_string());
    let result = BoxConfig::parse_for_tart(&spec).unwrap();
    assert!(matches!(result, BoxConfig::Snapshot(_)));
    if let BoxConfig::Snapshot(name) = result {
        assert_eq!(name, "snapshot-with-dashes_and_underscores.123");
    }
}

#[test]
fn test_tart_empty_string() {
    // Empty string should still be treated as a TartImage (even if invalid)
    let spec = BoxSpec::String("".to_string());
    let result = BoxConfig::parse_for_tart(&spec).unwrap();
    assert!(matches!(result, BoxConfig::TartImage(_)));
}

// =============================================================================
// Snapshot Tests (Common to Both)
// =============================================================================

#[test]
fn test_snapshot_prefix_stripping() {
    // Vagrant
    let spec = BoxSpec::String("@test-snapshot".to_string());
    let vagrant_result = BoxConfig::parse_for_vagrant(&spec).unwrap();
    if let BoxConfig::Snapshot(name) = vagrant_result {
        assert_eq!(name, "test-snapshot");
        assert!(!name.starts_with('@'));
    } else {
        panic!("Expected Snapshot variant");
    }

    // Tart
    let tart_result = BoxConfig::parse_for_tart(&spec).unwrap();
    if let BoxConfig::Snapshot(name) = tart_result {
        assert_eq!(name, "test-snapshot");
        assert!(!name.starts_with('@'));
    } else {
        panic!("Expected Snapshot variant");
    }
}

#[test]
fn test_snapshot_with_only_at_symbol() {
    // Edge case: "@" with nothing after it
    let spec = BoxSpec::String("@".to_string());

    // Both providers should treat this as a snapshot with empty name
    let vagrant_result = BoxConfig::parse_for_vagrant(&spec).unwrap();
    assert!(matches!(vagrant_result, BoxConfig::Snapshot(_)));
    if let BoxConfig::Snapshot(name) = vagrant_result {
        assert_eq!(name, "");
    }

    let tart_result = BoxConfig::parse_for_tart(&spec).unwrap();
    assert!(matches!(tart_result, BoxConfig::Snapshot(_)));
    if let BoxConfig::Snapshot(name) = tart_result {
        assert_eq!(name, "");
    }
}

// =============================================================================
// Build Spec Tests (Error Cases)
// =============================================================================

#[test]
fn test_vagrant_rejects_build_with_context() {
    let spec = BoxSpec::Build {
        dockerfile: "./Dockerfile".to_string(),
        context: Some("./context".to_string()),
        args: None,
    };
    let result = BoxConfig::parse_for_vagrant(&spec);
    assert!(result.is_err());
}

#[test]
fn test_vagrant_rejects_build_with_args() {
    use indexmap::IndexMap;
    let mut args = IndexMap::new();
    args.insert("ARG1".to_string(), "value1".to_string());

    let spec = BoxSpec::Build {
        dockerfile: "./Dockerfile".to_string(),
        context: None,
        args: Some(args),
    };
    let result = BoxConfig::parse_for_vagrant(&spec);
    assert!(result.is_err());
}

#[test]
fn test_tart_rejects_build_with_context() {
    let spec = BoxSpec::Build {
        dockerfile: "./Dockerfile".to_string(),
        context: Some("./context".to_string()),
        args: None,
    };
    let result = BoxConfig::parse_for_tart(&spec);
    assert!(result.is_err());
}

#[test]
fn test_tart_rejects_build_with_args() {
    use indexmap::IndexMap;
    let mut args = IndexMap::new();
    args.insert("ARG1".to_string(), "value1".to_string());

    let spec = BoxSpec::Build {
        dockerfile: "./Dockerfile".to_string(),
        context: None,
        args: Some(args),
    };
    let result = BoxConfig::parse_for_tart(&spec);
    assert!(result.is_err());
}
