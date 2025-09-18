#[cfg(test)]
mod tests {
    use crate::config::{MemoryLimit, VmConfig, VmSettings};
    use serde_yaml_ng as serde_yaml;

    #[test]
    fn test_numeric_memory_serialization() {
        let config = VmConfig {
            vm: Some(VmSettings {
                memory: Some(MemoryLimit::Limited(8192)),
                cpus: Some(4),
                user: Some("developer".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let yaml = serde_yaml::to_string(&config).unwrap();
        println!("Numeric memory YAML:\n{}", yaml);

        assert!(yaml.contains("memory: 8192"));
    }

    #[test]
    fn test_unlimited_memory_serialization() {
        let config = VmConfig {
            vm: Some(VmSettings {
                memory: Some(MemoryLimit::Unlimited),
                cpus: Some(4),
                user: Some("developer".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let yaml = serde_yaml::to_string(&config).unwrap();
        println!("Unlimited memory YAML:\n{}", yaml);

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
    fn test_docker_format_conversion() {
        let limited = MemoryLimit::Limited(8192);
        let unlimited = MemoryLimit::Unlimited;

        assert_eq!(limited.to_docker_format(), Some("8192m".to_string()));
        assert_eq!(unlimited.to_docker_format(), None);
    }
}
