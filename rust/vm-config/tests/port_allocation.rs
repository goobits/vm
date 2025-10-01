use vm_config::config::{ServiceConfig, VmConfig};

fn create_base_config() -> VmConfig {
    let mut config = VmConfig::default();
    config.ports.range = Some(vec![3100, 3109]); // 10 ports
    config
}

fn create_service(enabled: bool) -> ServiceConfig {
    ServiceConfig {
        enabled,
        ..Default::default()
    }
}

#[test]
fn test_scenario_new_project_auto_assigns_ports() {
    let mut config = create_base_config();
    config
        .services
        .insert("postgresql".to_string(), create_service(true));
    config
        .services
        .insert("redis".to_string(), create_service(true));

    config.ensure_service_ports();

    // Ports are allocated from the end of the range, in priority order.
    assert_eq!(config.services.get("postgresql").unwrap().port, Some(3109));
    assert_eq!(config.services.get("redis").unwrap().port, Some(3108));
}

#[test]
fn test_scenario_add_service_later() {
    let mut config = create_base_config();
    config
        .services
        .insert("postgresql".to_string(), create_service(true));
    config.ensure_service_ports();

    assert_eq!(config.services.get("postgresql").unwrap().port, Some(3109));

    // Now enable mongodb
    config
        .services
        .insert("mongodb".to_string(), create_service(true));
    config.ensure_service_ports();

    // Check existing and new port
    assert_eq!(config.services.get("postgresql").unwrap().port, Some(3109));
    assert_eq!(config.services.get("mongodb").unwrap().port, Some(3108));
}

#[test]
fn test_scenario_manual_port_override() {
    let mut config = create_base_config();
    let mut postgres_config = create_service(true);
    postgres_config.port = Some(5432); // Manual port outside range
    config
        .services
        .insert("postgresql".to_string(), postgres_config);
    config
        .services
        .insert("redis".to_string(), create_service(true));

    config.ensure_service_ports();

    // Manual port is preserved, redis gets auto-assigned from end
    assert_eq!(config.services.get("postgresql").unwrap().port, Some(5432));
    assert_eq!(config.services.get("redis").unwrap().port, Some(3109));
}

#[test]
fn test_scenario_disable_service_with_auto_port() {
    let mut config = create_base_config();
    config
        .services
        .insert("redis".to_string(), create_service(true));
    config.ensure_service_ports();

    assert_eq!(config.services.get("redis").unwrap().port, Some(3109));

    // Now disable redis
    config.services.get_mut("redis").unwrap().enabled = false;
    config.ensure_service_ports();

    // Port should be removed because it was in the auto-assigned range
    assert_eq!(config.services.get("redis").unwrap().port, None);
}

#[test]
fn test_scenario_disable_service_with_manual_port() {
    let mut config = create_base_config();
    let mut postgres_config = create_service(true);
    postgres_config.port = Some(9999); // Manual port outside range
    config
        .services
        .insert("postgresql".to_string(), postgres_config);
    config.ensure_service_ports();

    assert_eq!(config.services.get("postgresql").unwrap().port, Some(9999));

    // Now disable postgresql
    config.services.get_mut("postgresql").unwrap().enabled = false;
    config.ensure_service_ports();

    // Port should be preserved because it was outside the auto-assigned range
    assert_eq!(config.services.get("postgresql").unwrap().port, Some(9999));
}

#[test]
fn test_priority_order() {
    let mut config = create_base_config();
    // Add services in non-priority order
    config
        .services
        .insert("mongodb".to_string(), create_service(true));
    config
        .services
        .insert("redis".to_string(), create_service(true));
    config
        .services
        .insert("mysql".to_string(), create_service(true));
    config
        .services
        .insert("postgresql".to_string(), create_service(true));

    config.ensure_service_ports();

    // Priority order: postgresql, redis, mysql, mongodb (allocated from end)
    assert_eq!(config.services.get("postgresql").unwrap().port, Some(3109));
    assert_eq!(config.services.get("redis").unwrap().port, Some(3108));
    assert_eq!(config.services.get("mysql").unwrap().port, Some(3107));
    assert_eq!(config.services.get("mongodb").unwrap().port, Some(3106));
}

#[test]
fn test_port_conflict_avoidance() {
    let mut config = create_base_config();

    // Manually set a port near the end of the range
    let mut postgres_config = create_service(true);
    postgres_config.port = Some(3109);
    config
        .services
        .insert("postgresql".to_string(), postgres_config);

    // Add redis which should get auto-assigned
    config
        .services
        .insert("redis".to_string(), create_service(true));

    config.ensure_service_ports();

    // Manual port preserved, redis skips it and gets next available (going backwards)
    assert_eq!(config.services.get("postgresql").unwrap().port, Some(3109));
    assert_eq!(config.services.get("redis").unwrap().port, Some(3108));
}

#[test]
fn test_no_port_range_defined() {
    let mut config = VmConfig::default();
    config
        .services
        .insert("postgresql".to_string(), create_service(true));
    config
        .services
        .insert("redis".to_string(), create_service(true));

    config.ensure_service_ports();

    // Without a range, no ports should be assigned
    assert_eq!(config.services.get("postgresql").unwrap().port, None);
    assert_eq!(config.services.get("redis").unwrap().port, None);
}

#[test]
fn test_services_without_ports() {
    let mut config = create_base_config();
    config
        .services
        .insert("docker".to_string(), create_service(true));
    config
        .services
        .insert("postgresql".to_string(), create_service(true));

    config.ensure_service_ports();

    // Docker should not get a port (it's in SERVICES_WITHOUT_PORTS)
    assert_eq!(config.services.get("docker").unwrap().port, None);
    // PostgreSQL should get a port from end
    assert_eq!(config.services.get("postgresql").unwrap().port, Some(3109));
}

#[test]
fn test_port_exhaustion() {
    let mut config = VmConfig::default();
    config.ports.range = Some(vec![3100, 3102]); // Only 3 ports

    config
        .services
        .insert("postgresql".to_string(), create_service(true));
    config
        .services
        .insert("redis".to_string(), create_service(true));
    config
        .services
        .insert("mysql".to_string(), create_service(true));
    config
        .services
        .insert("mongodb".to_string(), create_service(true));

    config.ensure_service_ports();

    // First 3 get ports (from end)
    assert_eq!(config.services.get("postgresql").unwrap().port, Some(3102));
    assert_eq!(config.services.get("redis").unwrap().port, Some(3101));
    assert_eq!(config.services.get("mysql").unwrap().port, Some(3100));
    // Fourth service doesn't get a port (exhausted)
    assert_eq!(config.services.get("mongodb").unwrap().port, None);
}

#[test]
fn test_disabled_service_keeps_manual_port_in_range() {
    let mut config = create_base_config();
    let mut postgres_config = create_service(true);
    postgres_config.port = Some(3105); // Manual port INSIDE range
    config
        .services
        .insert("postgresql".to_string(), postgres_config);
    config.ensure_service_ports();

    assert_eq!(config.services.get("postgresql").unwrap().port, Some(3105));

    // Disable service
    config.services.get_mut("postgresql").unwrap().enabled = false;
    config.ensure_service_ports();

    // Port should be removed because it's within the range
    // (we can't distinguish manual vs auto-assigned ports within range)
    assert_eq!(config.services.get("postgresql").unwrap().port, None);
}
