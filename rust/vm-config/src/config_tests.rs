#[cfg(test)]
mod image_spec_tests {
    use crate::config::{ImageSpec, VmSettings};
    use indexmap::IndexMap;

    #[test]
    fn test_image_spec_string_deserialization() {
        let yaml = r#"
image: ubuntu:24.04
"#;
        let vm: VmSettings = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(matches!(vm.image, Some(ImageSpec::String(_))));
        if let Some(ImageSpec::String(s)) = vm.image {
            assert_eq!(s, "ubuntu:24.04");
        }
    }

    #[test]
    fn test_image_spec_string_deserialization_node() {
        let yaml = r#"
image: node:20
"#;
        let vm: VmSettings = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(matches!(vm.image, Some(ImageSpec::String(_))));
        if let Some(ImageSpec::String(s)) = vm.image {
            assert_eq!(s, "node:20");
        }
    }

    #[test]
    fn test_image_spec_dockerfile_path_deserialization() {
        let yaml = r#"
image: ./Dockerfile
"#;
        let vm: VmSettings = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(matches!(vm.image, Some(ImageSpec::String(_))));
        if let Some(ImageSpec::String(s)) = vm.image {
            assert_eq!(s, "./Dockerfile");
        }
    }

    #[test]
    fn test_image_spec_snapshot_deserialization() {
        let yaml = r#"
image: "@my-snapshot"
"#;
        let vm: VmSettings = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(matches!(vm.image, Some(ImageSpec::String(_))));
        if let Some(ImageSpec::String(s)) = vm.image {
            assert_eq!(s, "@my-snapshot");
        }
    }

    #[test]
    fn test_image_spec_build_deserialization_minimal() {
        let yaml = r#"
image:
  dockerfile: ./Dockerfile
"#;
        let vm: VmSettings = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(matches!(vm.image, Some(ImageSpec::Build { .. })));
        if let Some(ImageSpec::Build {
            dockerfile,
            context,
            args,
        }) = vm.image
        {
            assert_eq!(dockerfile, "./Dockerfile");
            assert_eq!(context, None);
            assert_eq!(args, None);
        }
    }

    #[test]
    fn test_image_spec_build_deserialization_with_context() {
        let yaml = r#"
image:
  dockerfile: ./Dockerfile
  context: .
"#;
        let vm: VmSettings = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(matches!(vm.image, Some(ImageSpec::Build { .. })));
        if let Some(ImageSpec::Build {
            dockerfile,
            context,
            args,
        }) = vm.image
        {
            assert_eq!(dockerfile, "./Dockerfile");
            assert_eq!(context, Some(".".to_string()));
            assert_eq!(args, None);
        }
    }

    #[test]
    fn test_image_spec_build_deserialization_full() {
        let yaml = r#"
image:
  dockerfile: ./Dockerfile
  context: .
  args:
    NODE_VERSION: "20"
    PYTHON_VERSION: "3.11"
"#;
        let vm: VmSettings = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(matches!(vm.image, Some(ImageSpec::Build { .. })));
        if let Some(ImageSpec::Build {
            dockerfile,
            context,
            args,
        }) = vm.image
        {
            assert_eq!(dockerfile, "./Dockerfile");
            assert_eq!(context, Some(".".to_string()));
            assert!(args.is_some());
            let args = args.unwrap();
            assert_eq!(args.get("NODE_VERSION"), Some(&"20".to_string()));
            assert_eq!(args.get("PYTHON_VERSION"), Some(&"3.11".to_string()));
        }
    }

    #[test]
    fn test_image_spec_build_with_nested_path() {
        let yaml = r#"
image:
  dockerfile: ./docker/app.dockerfile
  context: ./docker
"#;
        let vm: VmSettings = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(matches!(vm.image, Some(ImageSpec::Build { .. })));
        if let Some(ImageSpec::Build {
            dockerfile,
            context,
            ..
        }) = vm.image
        {
            assert_eq!(dockerfile, "./docker/app.dockerfile");
            assert_eq!(context, Some("./docker".to_string()));
        }
    }

    #[test]
    fn image_is_optional() {
        let yaml = r#"
user: myuser
memory: 4096
"#;
        let vm: VmSettings = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(vm.image.clone().is_none());
    }

    #[test]
    fn test_image_spec_partialeq_string() {
        let spec1 = ImageSpec::String("ubuntu:24.04".to_string());
        let spec2 = ImageSpec::String("ubuntu:24.04".to_string());
        let spec3 = ImageSpec::String("node:20".to_string());

        assert_eq!(spec1, spec2);
        assert_ne!(spec1, spec3);
    }

    #[test]
    fn test_image_spec_partialeq_build() {
        let spec1 = ImageSpec::Build {
            dockerfile: "./Dockerfile".to_string(),
            context: Some(".".to_string()),
            args: None,
        };
        let spec2 = ImageSpec::Build {
            dockerfile: "./Dockerfile".to_string(),
            context: Some(".".to_string()),
            args: None,
        };
        let spec3 = ImageSpec::Build {
            dockerfile: "./other.dockerfile".to_string(),
            context: Some(".".to_string()),
            args: None,
        };

        assert_eq!(spec1, spec2);
        assert_ne!(spec1, spec3);
    }

    #[test]
    fn test_image_spec_partialeq_build_with_args() {
        let mut args1 = IndexMap::new();
        args1.insert("NODE_VERSION".to_string(), "20".to_string());

        let mut args2 = IndexMap::new();
        args2.insert("NODE_VERSION".to_string(), "20".to_string());

        let spec1 = ImageSpec::Build {
            dockerfile: "./Dockerfile".to_string(),
            context: None,
            args: Some(args1.clone()),
        };
        let spec2 = ImageSpec::Build {
            dockerfile: "./Dockerfile".to_string(),
            context: None,
            args: Some(args2),
        };

        assert_eq!(spec1, spec2);
    }

    #[test]
    fn test_image_spec_partialeq_different_variants() {
        let spec1 = ImageSpec::String("./Dockerfile".to_string());
        let spec2 = ImageSpec::Build {
            dockerfile: "./Dockerfile".to_string(),
            context: None,
            args: None,
        };

        assert_ne!(spec1, spec2);
    }

    #[test]
    fn test_image_spec_serialization_string() {
        let spec = ImageSpec::String("ubuntu:24.04".to_string());
        let yaml = serde_yaml_ng::to_string(&spec).unwrap();
        assert_eq!(yaml.trim(), "ubuntu:24.04");
    }

    #[test]
    fn test_image_spec_serialization_build_minimal() {
        let spec = ImageSpec::Build {
            dockerfile: "./Dockerfile".to_string(),
            context: None,
            args: None,
        };
        let yaml = serde_yaml_ng::to_string(&spec).unwrap();
        assert!(yaml.contains("dockerfile: ./Dockerfile"));
        assert!(!yaml.contains("context:"));
        assert!(!yaml.contains("args:"));
    }

    #[test]
    fn test_image_spec_serialization_build_full() {
        let mut args = IndexMap::new();
        args.insert("NODE_VERSION".to_string(), "20".to_string());

        let spec = ImageSpec::Build {
            dockerfile: "./Dockerfile".to_string(),
            context: Some(".".to_string()),
            args: Some(args),
        };
        let yaml = serde_yaml_ng::to_string(&spec).unwrap();
        assert!(yaml.contains("dockerfile: ./Dockerfile"));
        assert!(yaml.contains("context: ."));
        assert!(yaml.contains("NODE_VERSION"));
    }

    #[test]
    fn test_image_spec_roundtrip_string() {
        let original = ImageSpec::String("ubuntu:24.04".to_string());
        let yaml = serde_yaml_ng::to_string(&original).unwrap();
        let deserialized: ImageSpec = serde_yaml_ng::from_str(&yaml).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_image_spec_roundtrip_build() {
        let mut args = IndexMap::new();
        args.insert("NODE_VERSION".to_string(), "20".to_string());

        let original = ImageSpec::Build {
            dockerfile: "./Dockerfile".to_string(),
            context: Some(".".to_string()),
            args: Some(args),
        };
        let yaml = serde_yaml_ng::to_string(&original).unwrap();
        let deserialized: ImageSpec = serde_yaml_ng::from_str(&yaml).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn vm_settings_reads_image() {
        let vm = VmSettings {
            image: Some(ImageSpec::String("node:20".to_string())),
            ..Default::default()
        };

        let spec = vm.image.clone().unwrap();
        assert_eq!(spec, ImageSpec::String("node:20".to_string()));
    }

    #[test]
    fn test_image_spec_clone() {
        let original = ImageSpec::String("ubuntu:24.04".to_string());
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    #[test]
    fn test_image_spec_debug() {
        let spec = ImageSpec::String("ubuntu:24.04".to_string());
        let debug = format!("{:?}", spec);
        assert!(debug.contains("ubuntu:24.04"));
    }
}
