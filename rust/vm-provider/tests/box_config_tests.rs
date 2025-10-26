use indexmap::IndexMap;
use std::path::PathBuf;
use vm_config::config::BoxSpec;
use vm_provider::BoxConfig;

/// Tests for BoxConfig::parse_for_docker() with string specifications
#[test]
fn test_docker_parses_image() {
    let spec = BoxSpec::String("ubuntu:24.04".to_string());
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from(".")).unwrap();
    assert!(matches!(result, BoxConfig::DockerImage(_)));
    if let BoxConfig::DockerImage(image) = result {
        assert_eq!(image, "ubuntu:24.04");
    }
}

#[test]
fn test_docker_parses_image_node() {
    let spec = BoxSpec::String("node:20".to_string());
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from(".")).unwrap();
    assert!(matches!(result, BoxConfig::DockerImage(_)));
    if let BoxConfig::DockerImage(image) = result {
        assert_eq!(image, "node:20");
    }
}

#[test]
fn test_docker_parses_image_with_registry() {
    let spec = BoxSpec::String("ghcr.io/user/myapp:latest".to_string());
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from(".")).unwrap();
    assert!(matches!(result, BoxConfig::DockerImage(_)));
    if let BoxConfig::DockerImage(image) = result {
        assert_eq!(image, "ghcr.io/user/myapp:latest");
    }
}

#[test]
fn test_docker_parses_dockerfile_path_relative() {
    let spec = BoxSpec::String("./Dockerfile".to_string());
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from("/workspace")).unwrap();
    assert!(matches!(result, BoxConfig::Dockerfile { .. }));
    if let BoxConfig::Dockerfile {
        path,
        context,
        args,
    } = result
    {
        assert_eq!(path, PathBuf::from("/workspace/Dockerfile"));
        assert_eq!(context, PathBuf::from("/workspace"));
        assert!(args.is_none());
    }
}

#[test]
fn test_docker_parses_dockerfile_path_parent() {
    let spec = BoxSpec::String("../Dockerfile".to_string());
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from("/workspace/sub")).unwrap();
    assert!(matches!(result, BoxConfig::Dockerfile { .. }));
    if let BoxConfig::Dockerfile { path, context, .. } = result {
        assert_eq!(path, PathBuf::from("/workspace/sub/../Dockerfile"));
        // Context is the parent of the path (not normalized)
        assert_eq!(context, PathBuf::from("/workspace/sub/.."));
    }
}

#[test]
fn test_docker_parses_dockerfile_path_absolute() {
    let spec = BoxSpec::String("/path/to/Dockerfile".to_string());
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from("/workspace")).unwrap();
    assert!(matches!(result, BoxConfig::Dockerfile { .. }));
    if let BoxConfig::Dockerfile { path, context, .. } = result {
        assert_eq!(path, PathBuf::from("/path/to/Dockerfile"));
        assert_eq!(context, PathBuf::from("/path/to"));
    }
}

#[test]
fn test_docker_parses_dockerfile_extension() {
    let spec = BoxSpec::String("app.dockerfile".to_string());
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from("/workspace")).unwrap();
    assert!(matches!(result, BoxConfig::Dockerfile { .. }));
    if let BoxConfig::Dockerfile { path, context, .. } = result {
        assert_eq!(path, PathBuf::from("/workspace/app.dockerfile"));
        assert_eq!(context, PathBuf::from("/workspace"));
    }
}

#[test]
fn test_docker_parses_dockerfile_extension_nested() {
    let spec = BoxSpec::String("docker/prod.dockerfile".to_string());
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from("/workspace")).unwrap();
    assert!(matches!(result, BoxConfig::Dockerfile { .. }));
    if let BoxConfig::Dockerfile { path, context, .. } = result {
        assert_eq!(path, PathBuf::from("/workspace/docker/prod.dockerfile"));
        assert_eq!(context, PathBuf::from("/workspace"));
    }
}

#[test]
fn test_docker_parses_snapshot() {
    let spec = BoxSpec::String("@my-snapshot".to_string());
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from(".")).unwrap();
    assert!(matches!(result, BoxConfig::Snapshot(_)));
    if let BoxConfig::Snapshot(name) = result {
        assert_eq!(name, "my-snapshot");
    }
}

#[test]
fn test_docker_parses_snapshot_complex_name() {
    let spec = BoxSpec::String("@dev-snapshot-2024-01-15".to_string());
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from(".")).unwrap();
    assert!(matches!(result, BoxConfig::Snapshot(_)));
    if let BoxConfig::Snapshot(name) = result {
        assert_eq!(name, "dev-snapshot-2024-01-15");
    }
}

#[test]
fn test_docker_parses_build_spec_minimal() {
    let spec = BoxSpec::Build {
        dockerfile: "./Dockerfile".to_string(),
        context: None,
        args: None,
    };
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from("/workspace")).unwrap();
    assert!(matches!(result, BoxConfig::Dockerfile { .. }));
    if let BoxConfig::Dockerfile {
        path,
        context,
        args,
    } = result
    {
        assert_eq!(path, PathBuf::from("/workspace/Dockerfile"));
        assert_eq!(context, PathBuf::from("/workspace"));
        assert!(args.is_none());
    }
}

#[test]
fn test_docker_parses_build_spec_with_context() {
    let spec = BoxSpec::Build {
        dockerfile: "./docker/Dockerfile".to_string(),
        context: Some("./docker".to_string()),
        args: None,
    };
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from("/workspace")).unwrap();
    assert!(matches!(result, BoxConfig::Dockerfile { .. }));
    if let BoxConfig::Dockerfile {
        path,
        context,
        args,
    } = result
    {
        assert_eq!(path, PathBuf::from("/workspace/docker/Dockerfile"));
        assert_eq!(context, PathBuf::from("/workspace/docker"));
        assert!(args.is_none());
    }
}

#[test]
fn test_docker_parses_build_spec_with_args() {
    let mut build_args = IndexMap::new();
    build_args.insert("NODE_VERSION".to_string(), "20".to_string());
    build_args.insert("PYTHON_VERSION".to_string(), "3.11".to_string());

    let spec = BoxSpec::Build {
        dockerfile: "./Dockerfile".to_string(),
        context: Some(".".to_string()),
        args: Some(build_args.clone()),
    };
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from("/workspace")).unwrap();
    assert!(matches!(result, BoxConfig::Dockerfile { .. }));
    if let BoxConfig::Dockerfile {
        path,
        context,
        args,
    } = result
    {
        assert_eq!(path, PathBuf::from("/workspace/Dockerfile"));
        assert_eq!(context, PathBuf::from("/workspace"));
        assert!(args.is_some());
        let args = args.unwrap();
        assert_eq!(args.get("NODE_VERSION"), Some(&"20".to_string()));
        assert_eq!(args.get("PYTHON_VERSION"), Some(&"3.11".to_string()));
    }
}

#[test]
fn test_docker_parses_build_spec_default_context() {
    let spec = BoxSpec::Build {
        dockerfile: "nested/dir/Dockerfile".to_string(),
        context: None,
        args: None,
    };
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from("/workspace")).unwrap();
    assert!(matches!(result, BoxConfig::Dockerfile { .. }));
    if let BoxConfig::Dockerfile { path, context, .. } = result {
        assert_eq!(path, PathBuf::from("/workspace/nested/dir/Dockerfile"));
        // Context should be the parent directory of the Dockerfile
        assert_eq!(context, PathBuf::from("/workspace/nested/dir"));
    }
}

#[test]
fn test_box_config_clone_docker_image() {
    let config = BoxConfig::DockerImage("ubuntu:24.04".to_string());
    let cloned = config.clone();
    assert!(matches!(cloned, BoxConfig::DockerImage(_)));
    if let BoxConfig::DockerImage(image) = cloned {
        assert_eq!(image, "ubuntu:24.04");
    }
}

#[test]
fn test_box_config_clone_dockerfile() {
    let config = BoxConfig::Dockerfile {
        path: PathBuf::from("/workspace/Dockerfile"),
        context: PathBuf::from("/workspace"),
        args: None,
    };
    let cloned = config.clone();
    assert!(matches!(cloned, BoxConfig::Dockerfile { .. }));
}

#[test]
fn test_box_config_clone_snapshot() {
    let config = BoxConfig::Snapshot("my-snapshot".to_string());
    let cloned = config.clone();
    assert!(matches!(cloned, BoxConfig::Snapshot(_)));
    if let BoxConfig::Snapshot(name) = cloned {
        assert_eq!(name, "my-snapshot");
    }
}

#[test]
fn test_box_config_debug_formatting() {
    let config = BoxConfig::DockerImage("ubuntu:24.04".to_string());
    let debug = format!("{:?}", config);
    assert!(debug.contains("DockerImage"));
    assert!(debug.contains("ubuntu:24.04"));
}

/// Edge case: String that looks like a path but is actually an image name
#[test]
fn test_docker_image_vs_path_disambiguation() {
    // These should be treated as images (no ./, ../, or / prefix)
    let spec = BoxSpec::String("myregistry.com/path/to/image:tag".to_string());
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from(".")).unwrap();
    assert!(matches!(result, BoxConfig::DockerImage(_)));

    // This should be treated as a Dockerfile path
    let spec = BoxSpec::String("./myregistry.com/Dockerfile".to_string());
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from(".")).unwrap();
    assert!(matches!(result, BoxConfig::Dockerfile { .. }));
}

/// Test that @ prefix is always interpreted as snapshot
#[test]
fn test_snapshot_prefix_always_wins() {
    let spec = BoxSpec::String("@ubuntu:24.04".to_string());
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from(".")).unwrap();
    assert!(matches!(result, BoxConfig::Snapshot(_)));
    if let BoxConfig::Snapshot(name) = result {
        // The @ is stripped, rest is kept as-is
        assert_eq!(name, "ubuntu:24.04");
    }
}

/// Test .dockerfile extension detection regardless of case
#[test]
fn test_dockerfile_extension_case_sensitive() {
    // Lowercase should work
    let spec = BoxSpec::String("app.dockerfile".to_string());
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from(".")).unwrap();
    assert!(matches!(result, BoxConfig::Dockerfile { .. }));

    // Uppercase or mixed case won't match (Unix-like filesystems are case-sensitive)
    let spec = BoxSpec::String("app.Dockerfile".to_string());
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from(".")).unwrap();
    // This will be treated as an image name since it doesn't end with .dockerfile
    assert!(matches!(result, BoxConfig::DockerImage(_)));
}

/// Test build args conversion from IndexMap to HashMap
#[test]
fn test_build_args_conversion() {
    let mut build_args = IndexMap::new();
    build_args.insert("ARG1".to_string(), "value1".to_string());
    build_args.insert("ARG2".to_string(), "value2".to_string());

    let spec = BoxSpec::Build {
        dockerfile: "./Dockerfile".to_string(),
        context: Some(".".to_string()),
        args: Some(build_args),
    };

    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from("/workspace")).unwrap();
    if let BoxConfig::Dockerfile { args, .. } = result {
        let args = args.unwrap();
        // Verify both args are present
        assert_eq!(args.len(), 2);
        assert!(args.contains_key("ARG1"));
        assert!(args.contains_key("ARG2"));
    }
}

/// Test empty string handling
#[test]
fn test_empty_string_is_docker_image() {
    let spec = BoxSpec::String("".to_string());
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from(".")).unwrap();
    // Empty string is treated as a Docker image (albeit invalid)
    assert!(matches!(result, BoxConfig::DockerImage(_)));
}

/// Test path normalization with base_dir
#[test]
fn test_path_joins_with_base_dir() {
    let spec = BoxSpec::String("./custom.dockerfile".to_string());
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from("/home/user/project")).unwrap();
    if let BoxConfig::Dockerfile { path, context, .. } = result {
        assert_eq!(path, PathBuf::from("/home/user/project/custom.dockerfile"));
        assert_eq!(context, PathBuf::from("/home/user/project"));
    }
}

/// Test build spec with absolute dockerfile path
/// Note: The current implementation joins all dockerfile paths with base_dir,
/// even absolute paths. This test documents this behavior.
#[test]
fn test_build_spec_absolute_dockerfile() {
    let spec = BoxSpec::Build {
        dockerfile: "/absolute/path/Dockerfile".to_string(),
        context: Some(".".to_string()),
        args: None,
    };
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from("/workspace")).unwrap();
    if let BoxConfig::Dockerfile { path, context, .. } = result {
        // Absolute paths should be preserved
        assert_eq!(path, PathBuf::from("/absolute/path/Dockerfile"));
        // Relative context is joined with base_dir
        assert_eq!(context, PathBuf::from("/workspace/."));
    }
}

#[test]
fn test_build_spec_absolute_context() {
    let spec = BoxSpec::Build {
        dockerfile: "./Dockerfile".to_string(),
        context: Some("/absolute/context".to_string()),
        args: None,
    };
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from("/workspace")).unwrap();
    if let BoxConfig::Dockerfile { path, context, .. } = result {
        // Relative dockerfile is joined with base_dir
        assert_eq!(path, PathBuf::from("/workspace/./Dockerfile"));
        // Absolute context should be preserved
        assert_eq!(context, PathBuf::from("/absolute/context"));
    }
}

#[cfg(target_os = "windows")]
#[test]
fn test_windows_absolute_path() {
    let spec = BoxSpec::String(r"C:\workspace\Dockerfile".to_string());
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from(r"D:\project")).unwrap();
    if let BoxConfig::Dockerfile { path, .. } = result {
        // Windows absolute path should be preserved
        assert_eq!(path, PathBuf::from(r"C:\workspace\Dockerfile"));
    }
}

#[cfg(target_os = "windows")]
#[test]
fn test_windows_absolute_build_spec() {
    let spec = BoxSpec::Build {
        dockerfile: r"C:\workspace\Dockerfile".to_string(),
        context: Some(r"D:\context".to_string()),
        args: None,
    };
    let result = BoxConfig::parse_for_docker(&spec, &PathBuf::from(r"E:\base")).unwrap();
    if let BoxConfig::Dockerfile { path, context, .. } = result {
        // Both absolute paths should be preserved
        assert_eq!(path, PathBuf::from(r"C:\workspace\Dockerfile"));
        assert_eq!(context, PathBuf::from(r"D:\context"));
    }
}
