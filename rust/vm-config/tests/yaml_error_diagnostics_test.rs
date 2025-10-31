use std::fs;
use tempfile::TempDir;
use vm_config::yaml::CoreOperations;

#[test]
fn test_duplicate_key_detection() {
    let yaml_with_duplicate = r#"
version: "2.0"
provider: docker
npm_packages:
  - '@anthropic-ai/claude-code'
  - '@google/gemini-cli'
npm_packages:
  - '@anthropic-ai/claude-code'
  - '@google/gemini-cli'
  - '@openai/codex'
"#;

    let result: Result<serde_yaml_ng::Value, _> =
        CoreOperations::parse_yaml_with_diagnostics(yaml_with_duplicate, "test.yaml");

    assert!(result.is_err(), "Should detect duplicate keys");
    let error_msg = result.unwrap_err().to_string();

    // Check that the error message mentions the duplicate key
    assert!(
        error_msg.contains("npm_packages")
            || error_msg.contains("duplicate")
            || error_msg.contains("Duplicate"),
        "Error should mention duplicate keys or npm_packages, got: {}",
        error_msg
    );
}

#[test]
fn test_valid_yaml_parses_successfully() {
    let valid_yaml = r#"
version: "2.0"
provider: docker
npm_packages:
  - '@anthropic-ai/claude-code'
  - '@google/gemini-cli'
  - '@openai/codex'
"#;

    let result: Result<serde_yaml_ng::Value, _> =
        CoreOperations::parse_yaml_with_diagnostics(valid_yaml, "test.yaml");

    assert!(result.is_ok(), "Valid YAML should parse successfully");
}

#[test]
fn test_file_with_duplicate_keys() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("duplicate.yaml");

    let yaml_content = r#"
version: "2.0"
provider: docker
ports:
  web: 3000
ports:
  api: 8080
"#;

    fs::write(&file_path, yaml_content).unwrap();

    let result = CoreOperations::validate_file(&file_path);

    assert!(result.is_err(), "Should detect duplicate 'ports' key");
    let error_msg = result.unwrap_err().to_string();

    assert!(
        error_msg.contains("ports")
            || error_msg.contains("duplicate")
            || error_msg.contains("Duplicate"),
        "Error should mention duplicate or ports, got: {}",
        error_msg
    );
}
