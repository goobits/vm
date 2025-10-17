#[cfg(test)]
mod tests {
    use crate::config::{CpuLimit, MemoryLimit, VmConfig, VmSettings};
    use serde_yaml_ng as serde_yaml;

    // ===== Memory Limit Tests =====

    #[test]
    fn test_numeric_memory_serialization() {
        let config = VmConfig {
            vm: Some(VmSettings {
                memory: Some(MemoryLimit::Limited(8192)),
                cpus: Some(CpuLimit::Limited(4)),
                user: Some("developer".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.contains("memory: 8192"));
    }

    #[test]
    fn test_percentage_memory_serialization() {
        let config = VmConfig {
            vm: Some(VmSettings {
                memory: Some(MemoryLimit::Percentage(50)),
                cpus: Some(CpuLimit::Limited(4)),
                user: Some("developer".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.contains("memory: 50%"));
    }

    #[test]
    fn test_unlimited_memory_serialization() {
        let config = VmConfig {
            vm: Some(VmSettings {
                memory: Some(MemoryLimit::Unlimited),
                cpus: Some(CpuLimit::Limited(4)),
                user: Some("developer".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.contains("memory: unlimited"));
    }

    #[test]
    fn test_numeric_memory_deserialization() {
        let yaml = r#"
vm:
  memory: 8192
  cpus: 4
  user: developer
"#;

        let config: VmConfig = serde_yaml::from_str(yaml).unwrap();

        assert!(config.vm.is_some());
        let vm = config.vm.unwrap();
        assert!(vm.memory.is_some());
        let memory = vm.memory.unwrap();
        assert_eq!(memory.to_mb(), Some(8192));
        assert!(!memory.is_unlimited());
    }

    #[test]
    fn test_memory_units_deserialization() {
        // Test GB
        let yaml_gb = r#"
vm:
  memory: "1gb"
  user: developer
"#;
        let config: VmConfig = serde_yaml::from_str(yaml_gb).unwrap();
        let memory = config.vm.unwrap().memory.unwrap();
        assert_eq!(memory.to_mb(), Some(1024)); // 1GB = 1024MB

        // Test MB
        let yaml_mb = r#"
vm:
  memory: "512mb"
  user: developer
"#;
        let config: VmConfig = serde_yaml::from_str(yaml_mb).unwrap();
        let memory = config.vm.unwrap().memory.unwrap();
        assert_eq!(memory.to_mb(), Some(512));

        // Test case insensitivity
        let yaml_gb_caps = r#"
vm:
  memory: "2GB"
  user: developer
"#;
        let config: VmConfig = serde_yaml::from_str(yaml_gb_caps).unwrap();
        let memory = config.vm.unwrap().memory.unwrap();
        assert_eq!(memory.to_mb(), Some(2048));
    }

    #[test]
    fn test_percentage_memory_deserialization() {
        let yaml = r#"
vm:
  memory: "50%"
  cpus: 4
  user: developer
"#;

        let config: VmConfig = serde_yaml::from_str(yaml).unwrap();

        assert!(config.vm.is_some());
        let vm = config.vm.unwrap();
        assert!(vm.memory.is_some());
        let memory = vm.memory.unwrap();
        assert!(memory.is_percentage());
        assert_eq!(memory.to_percentage(), Some(50));
        assert_eq!(memory.to_mb(), None); // Percentages don't have direct MB value
    }

    #[test]
    fn test_unlimited_memory_deserialization() {
        let yaml = r#"
vm:
  memory: "unlimited"
  cpus: 4
  user: developer
"#;

        let config: VmConfig = serde_yaml::from_str(yaml).unwrap();

        assert!(config.vm.is_some());
        let vm = config.vm.unwrap();
        assert!(vm.memory.is_some());
        let memory = vm.memory.unwrap();
        assert_eq!(memory.to_mb(), None);
        assert!(memory.is_unlimited());
    }

    #[test]
    fn test_memory_percentage_resolution() {
        let percent_50 = MemoryLimit::Percentage(50);
        let percent_90 = MemoryLimit::Percentage(90);
        let limited = MemoryLimit::Limited(4096);

        // 50% of 16GB = 8GB
        assert_eq!(percent_50.resolve_percentage(16384), Some(8192));
        // 90% of 8GB = 7.2GB
        assert_eq!(percent_90.resolve_percentage(8192), Some(7372));
        // Limited just returns the value
        assert_eq!(limited.resolve_percentage(16384), Some(4096));
    }

    #[test]
    fn test_docker_format_conversion() {
        let limited = MemoryLimit::Limited(8192);
        let percentage = MemoryLimit::Percentage(50);
        let unlimited = MemoryLimit::Unlimited;

        assert_eq!(limited.to_docker_format(), Some("8192m".to_string()));
        assert_eq!(percentage.to_docker_format(), None); // Can't convert percentage without context
        assert_eq!(unlimited.to_docker_format(), None);
    }

    // ===== CPU Limit Tests =====

    #[test]
    fn test_numeric_cpu_serialization() {
        let config = VmConfig {
            vm: Some(VmSettings {
                memory: Some(MemoryLimit::Limited(4096)),
                cpus: Some(CpuLimit::Limited(4)),
                user: Some("developer".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.contains("cpus: 4"));
    }

    #[test]
    fn test_percentage_cpu_serialization() {
        let config = VmConfig {
            vm: Some(VmSettings {
                memory: Some(MemoryLimit::Limited(4096)),
                cpus: Some(CpuLimit::Percentage(50)),
                user: Some("developer".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.contains("cpus: 50%"));
    }

    #[test]
    fn test_unlimited_cpu_serialization() {
        let config = VmConfig {
            vm: Some(VmSettings {
                memory: Some(MemoryLimit::Limited(4096)),
                cpus: Some(CpuLimit::Unlimited),
                user: Some("developer".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.contains("cpus: unlimited"));
    }

    #[test]
    fn test_numeric_cpu_deserialization() {
        let yaml = r#"
vm:
  memory: 8192
  cpus: 4
  user: developer
"#;

        let config: VmConfig = serde_yaml::from_str(yaml).unwrap();

        let cpus = config.vm.unwrap().cpus.unwrap();
        assert_eq!(cpus.to_count(), Some(4));
        assert!(!cpus.is_unlimited());
    }

    #[test]
    fn test_percentage_cpu_deserialization() {
        let yaml = r#"
vm:
  memory: 8192
  cpus: "75%"
  user: developer
"#;

        let config: VmConfig = serde_yaml::from_str(yaml).unwrap();

        let cpus = config.vm.unwrap().cpus.unwrap();
        assert!(cpus.is_percentage());
        assert_eq!(cpus.to_percentage(), Some(75));
        assert_eq!(cpus.to_count(), None); // Percentages don't have direct count
    }

    #[test]
    fn test_unlimited_cpu_deserialization() {
        let yaml = r#"
vm:
  memory: 8192
  cpus: "unlimited"
  user: developer
"#;

        let config: VmConfig = serde_yaml::from_str(yaml).unwrap();

        let cpus = config.vm.unwrap().cpus.unwrap();
        assert!(cpus.is_unlimited());
        assert_eq!(cpus.to_count(), None);
    }

    #[test]
    fn test_cpu_percentage_resolution() {
        let percent_50 = CpuLimit::Percentage(50);
        let percent_25 = CpuLimit::Percentage(25);
        let limited = CpuLimit::Limited(4);

        // 50% of 8 CPUs = 4 CPUs
        assert_eq!(percent_50.resolve_percentage(8), Some(4));
        // 25% of 8 CPUs = 2 CPUs
        assert_eq!(percent_25.resolve_percentage(8), Some(2));
        // Limited just returns the value
        assert_eq!(limited.resolve_percentage(16), Some(4));
        // Always at least 1 CPU
        assert_eq!(percent_25.resolve_percentage(2), Some(1));
    }

    #[test]
    fn test_cpu_rejects_memory_units() {
        let yaml = r#"
vm:
  memory: 8192
  cpus: "1gb"
  user: developer
"#;

        let result: Result<VmConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Memory units") || err.contains("not valid for CPU"));
    }

    // ===== Round-trip Tests =====

    #[test]
    fn test_memory_percentage_roundtrip() {
        let original = VmConfig {
            vm: Some(VmSettings {
                memory: Some(MemoryLimit::Percentage(75)),
                cpus: Some(CpuLimit::Limited(4)),
                user: Some("developer".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let yaml = serde_yaml::to_string(&original).unwrap();
        let parsed: VmConfig = serde_yaml::from_str(&yaml).unwrap();

        assert_eq!(
            parsed.vm.as_ref().unwrap().memory.as_ref().unwrap(),
            &MemoryLimit::Percentage(75)
        );
    }

    #[test]
    fn test_cpu_percentage_roundtrip() {
        let original = VmConfig {
            vm: Some(VmSettings {
                memory: Some(MemoryLimit::Limited(4096)),
                cpus: Some(CpuLimit::Percentage(50)),
                user: Some("developer".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let yaml = serde_yaml::to_string(&original).unwrap();
        let parsed: VmConfig = serde_yaml::from_str(&yaml).unwrap();

        assert_eq!(
            parsed.vm.as_ref().unwrap().cpus.as_ref().unwrap(),
            &CpuLimit::Percentage(50)
        );
    }
}
