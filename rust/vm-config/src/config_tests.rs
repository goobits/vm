#[cfg(test)]
mod box_spec_tests {
    use crate::config::{BoxSpec, VmSettings};
    use indexmap::IndexMap;

    #[test]
    fn test_box_spec_string_deserialization() {
        let yaml = r#"
box: ubuntu:24.04
"#;
        let vm: VmSettings = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(matches!(vm.r#box, Some(BoxSpec::String(_))));
        if let Some(BoxSpec::String(s)) = vm.r#box {
            assert_eq!(s, "ubuntu:24.04");
        }
    }

    #[test]
    fn test_box_spec_string_deserialization_node() {
        let yaml = r#"
box: node:20
"#;
        let vm: VmSettings = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(matches!(vm.r#box, Some(BoxSpec::String(_))));
        if let Some(BoxSpec::String(s)) = vm.r#box {
            assert_eq!(s, "node:20");
        }
    }

    #[test]
    fn test_box_spec_dockerfile_path_deserialization() {
        let yaml = r#"
box: ./Dockerfile
"#;
        let vm: VmSettings = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(matches!(vm.r#box, Some(BoxSpec::String(_))));
        if let Some(BoxSpec::String(s)) = vm.r#box {
            assert_eq!(s, "./Dockerfile");
        }
    }

    #[test]
    fn test_box_spec_snapshot_deserialization() {
        let yaml = r#"
box: "@my-snapshot"
"#;
        let vm: VmSettings = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(matches!(vm.r#box, Some(BoxSpec::String(_))));
        if let Some(BoxSpec::String(s)) = vm.r#box {
            assert_eq!(s, "@my-snapshot");
        }
    }

    #[test]
    fn test_box_spec_build_deserialization_minimal() {
        let yaml = r#"
box:
  dockerfile: ./Dockerfile
"#;
        let vm: VmSettings = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(matches!(vm.r#box, Some(BoxSpec::Build { .. })));
        if let Some(BoxSpec::Build {
            dockerfile,
            context,
            args,
        }) = vm.r#box
        {
            assert_eq!(dockerfile, "./Dockerfile");
            assert_eq!(context, None);
            assert_eq!(args, None);
        }
    }

    #[test]
    fn test_box_spec_build_deserialization_with_context() {
        let yaml = r#"
box:
  dockerfile: ./Dockerfile
  context: .
"#;
        let vm: VmSettings = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(matches!(vm.r#box, Some(BoxSpec::Build { .. })));
        if let Some(BoxSpec::Build {
            dockerfile,
            context,
            args,
        }) = vm.r#box
        {
            assert_eq!(dockerfile, "./Dockerfile");
            assert_eq!(context, Some(".".to_string()));
            assert_eq!(args, None);
        }
    }

    #[test]
    fn test_box_spec_build_deserialization_full() {
        let yaml = r#"
box:
  dockerfile: ./Dockerfile
  context: .
  args:
    NODE_VERSION: "20"
    PYTHON_VERSION: "3.11"
"#;
        let vm: VmSettings = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(matches!(vm.r#box, Some(BoxSpec::Build { .. })));
        if let Some(BoxSpec::Build {
            dockerfile,
            context,
            args,
        }) = vm.r#box
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
    fn test_box_spec_build_with_nested_path() {
        let yaml = r#"
box:
  dockerfile: ./docker/app.dockerfile
  context: ./docker
"#;
        let vm: VmSettings = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(matches!(vm.r#box, Some(BoxSpec::Build { .. })));
        if let Some(BoxSpec::Build {
            dockerfile,
            context,
            ..
        }) = vm.r#box
        {
            assert_eq!(dockerfile, "./docker/app.dockerfile");
            assert_eq!(context, Some("./docker".to_string()));
        }
    }

    #[test]
    fn test_backwards_compat_box_name() {
        let yaml = "box_name: ubuntu:24.04";
        let vm: VmSettings = serde_yaml_ng::from_str(yaml).unwrap();
        let box_spec = vm.get_box_spec().unwrap();
        assert_eq!(box_spec, BoxSpec::String("ubuntu:24.04".to_string()));
    }

    #[test]
    fn test_backwards_compat_box_name_node() {
        let yaml = "box_name: node:20-alpine";
        let vm: VmSettings = serde_yaml_ng::from_str(yaml).unwrap();
        let box_spec = vm.get_box_spec().unwrap();
        assert_eq!(box_spec, BoxSpec::String("node:20-alpine".to_string()));
    }

    #[test]
    fn test_box_takes_precedence_over_box_name() {
        let yaml = r#"
box: node:20
box_name: ubuntu:24.04
"#;
        let vm: VmSettings = serde_yaml_ng::from_str(yaml).unwrap();
        let box_spec = vm.get_box_spec().unwrap();
        assert_eq!(box_spec, BoxSpec::String("node:20".to_string()));
    }

    #[test]
    fn test_box_build_takes_precedence_over_box_name() {
        let yaml = r#"
box:
  dockerfile: ./Dockerfile
  context: .
box_name: ubuntu:24.04
"#;
        let vm: VmSettings = serde_yaml_ng::from_str(yaml).unwrap();
        let box_spec = vm.get_box_spec().unwrap();
        assert!(matches!(box_spec, BoxSpec::Build { .. }));
        if let BoxSpec::Build { dockerfile, .. } = box_spec {
            assert_eq!(dockerfile, "./Dockerfile");
        }
    }

    #[test]
    fn test_get_box_spec_returns_none_when_both_missing() {
        let yaml = r#"
user: myuser
memory: 4096
"#;
        let vm: VmSettings = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(vm.get_box_spec().is_none());
    }

    #[test]
    fn test_box_spec_partialeq_string() {
        let spec1 = BoxSpec::String("ubuntu:24.04".to_string());
        let spec2 = BoxSpec::String("ubuntu:24.04".to_string());
        let spec3 = BoxSpec::String("node:20".to_string());

        assert_eq!(spec1, spec2);
        assert_ne!(spec1, spec3);
    }

    #[test]
    fn test_box_spec_partialeq_build() {
        let spec1 = BoxSpec::Build {
            dockerfile: "./Dockerfile".to_string(),
            context: Some(".".to_string()),
            args: None,
        };
        let spec2 = BoxSpec::Build {
            dockerfile: "./Dockerfile".to_string(),
            context: Some(".".to_string()),
            args: None,
        };
        let spec3 = BoxSpec::Build {
            dockerfile: "./other.dockerfile".to_string(),
            context: Some(".".to_string()),
            args: None,
        };

        assert_eq!(spec1, spec2);
        assert_ne!(spec1, spec3);
    }

    #[test]
    fn test_box_spec_partialeq_build_with_args() {
        let mut args1 = IndexMap::new();
        args1.insert("NODE_VERSION".to_string(), "20".to_string());

        let mut args2 = IndexMap::new();
        args2.insert("NODE_VERSION".to_string(), "20".to_string());

        let spec1 = BoxSpec::Build {
            dockerfile: "./Dockerfile".to_string(),
            context: None,
            args: Some(args1.clone()),
        };
        let spec2 = BoxSpec::Build {
            dockerfile: "./Dockerfile".to_string(),
            context: None,
            args: Some(args2),
        };

        assert_eq!(spec1, spec2);
    }

    #[test]
    fn test_box_spec_partialeq_different_variants() {
        let spec1 = BoxSpec::String("./Dockerfile".to_string());
        let spec2 = BoxSpec::Build {
            dockerfile: "./Dockerfile".to_string(),
            context: None,
            args: None,
        };

        assert_ne!(spec1, spec2);
    }

    #[test]
    fn test_box_spec_serialization_string() {
        let spec = BoxSpec::String("ubuntu:24.04".to_string());
        let yaml = serde_yaml_ng::to_string(&spec).unwrap();
        assert_eq!(yaml.trim(), "ubuntu:24.04");
    }

    #[test]
    fn test_box_spec_serialization_build_minimal() {
        let spec = BoxSpec::Build {
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
    fn test_box_spec_serialization_build_full() {
        let mut args = IndexMap::new();
        args.insert("NODE_VERSION".to_string(), "20".to_string());

        let spec = BoxSpec::Build {
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
    fn test_box_spec_roundtrip_string() {
        let original = BoxSpec::String("ubuntu:24.04".to_string());
        let yaml = serde_yaml_ng::to_string(&original).unwrap();
        let deserialized: BoxSpec = serde_yaml_ng::from_str(&yaml).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_box_spec_roundtrip_build() {
        let mut args = IndexMap::new();
        args.insert("NODE_VERSION".to_string(), "20".to_string());

        let original = BoxSpec::Build {
            dockerfile: "./Dockerfile".to_string(),
            context: Some(".".to_string()),
            args: Some(args),
        };
        let yaml = serde_yaml_ng::to_string(&original).unwrap();
        let deserialized: BoxSpec = serde_yaml_ng::from_str(&yaml).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_vm_settings_get_box_spec_prefers_box() {
        let vm = VmSettings {
            r#box: Some(BoxSpec::String("node:20".to_string())),
            box_name: Some("ubuntu:24.04".to_string()),
            ..Default::default()
        };

        let spec = vm.get_box_spec().unwrap();
        assert_eq!(spec, BoxSpec::String("node:20".to_string()));
    }

    #[test]
    fn test_vm_settings_get_box_spec_falls_back_to_box_name() {
        let vm = VmSettings {
            r#box: None,
            box_name: Some("ubuntu:24.04".to_string()),
            ..Default::default()
        };

        let spec = vm.get_box_spec().unwrap();
        assert_eq!(spec, BoxSpec::String("ubuntu:24.04".to_string()));
    }

    #[test]
    fn test_vm_settings_get_box_spec_returns_none() {
        let vm = VmSettings {
            r#box: None,
            box_name: None,
            ..Default::default()
        };

        assert!(vm.get_box_spec().is_none());
    }

    #[test]
    fn test_box_spec_clone() {
        let original = BoxSpec::String("ubuntu:24.04".to_string());
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    #[test]
    fn test_box_spec_debug() {
        let spec = BoxSpec::String("ubuntu:24.04".to_string());
        let debug = format!("{:?}", spec);
        assert!(debug.contains("ubuntu:24.04"));
    }
}
