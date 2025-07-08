#!/bin/bash
# Test Suite: Service Toggle Tests
# Tests that services can be enabled/disabled independently

# Test PostgreSQL only
test_postgresql_only() {
    echo "Testing PostgreSQL service in isolation..."
    
    create_test_vm "$CONFIG_DIR/services/postgresql.json" || return 1
    
    # PostgreSQL should be installed
    assert_service_enabled "postgresql" "PostgreSQL should be installed"
    
    # Other services should NOT be installed
    assert_service_not_enabled "redis" "Redis should not be installed"
    assert_service_not_enabled "mongodb" "MongoDB should not be installed"
    
    # Test PostgreSQL functionality
    assert_command_succeeds "sudo -u postgres psql -c 'SELECT version();'" "PostgreSQL query works"
    assert_output_contains "sudo -u postgres psql -c 'SELECT version();'" "PostgreSQL" "PostgreSQL version check"
}

# Test Redis only
test_redis_only() {
    echo "Testing Redis service in isolation..."
    
    create_test_vm "$CONFIG_DIR/services/redis.json" || return 1
    
    # Redis should be installed
    assert_service_enabled "redis" "Redis should be installed"
    
    # Other services should NOT be installed
    assert_service_not_enabled "postgresql" "PostgreSQL should not be installed"
    assert_service_not_enabled "mongodb" "MongoDB should not be installed"
    
    # Test Redis functionality
    assert_command_succeeds "redis-cli ping" "Redis ping works"
    assert_output_contains "redis-cli ping" "PONG" "Redis responds to ping"
}

# Test MongoDB only
test_mongodb_only() {
    echo "Testing MongoDB service in isolation..."
    
    create_test_vm "$CONFIG_DIR/services/mongodb.json" || return 1
    
    # MongoDB should be installed
    assert_service_enabled "mongodb" "MongoDB should be installed"
    
    # Other services should NOT be installed
    assert_service_not_enabled "postgresql" "PostgreSQL should not be installed"
    assert_service_not_enabled "redis" "Redis should not be installed"
    
    # Test MongoDB functionality
    assert_command_succeeds "mongosh --eval 'db.version()'" "MongoDB query works"
}

# Test Docker only
test_docker_only() {
    echo "Testing Docker service in isolation..."
    
    create_test_vm "$CONFIG_DIR/services/docker.json" || return 1
    
    # Docker should be installed
    assert_service_enabled "docker" "Docker should be installed"
    
    # Other services should NOT be installed
    assert_service_not_enabled "postgresql" "PostgreSQL should not be installed"
    assert_service_not_enabled "redis" "Redis should not be installed"
    
    # Test Docker functionality
    assert_command_succeeds "docker --version" "Docker version check"
    assert_command_succeeds "docker run hello-world" "Docker can run containers"
}

# Test multiple services enabled
test_multiple_services() {
    echo "Testing multiple services enabled together..."
    
    # Generate config with PostgreSQL and Redis enabled
    local multi_config="/tmp/multi-services-$$.json"
    generate_config "multi-services" '{
        "project": {"name": "test-multi-services"},
        "services": {
            "postgresql": {"enabled": true},
            "redis": {"enabled": true},
            "mongodb": {"enabled": false},
            "docker": {"enabled": false}
        }
    }' "$multi_config"
    
    create_test_vm "$multi_config" || return 1
    
    # Check enabled services
    assert_service_enabled "postgresql" "PostgreSQL should be installed"
    assert_service_enabled "redis" "Redis should be installed"
    
    # Check disabled services
    assert_service_not_enabled "mongodb" "MongoDB should not be installed"
    assert_service_not_enabled "docker" "Docker should not be installed"
    
    # Test both services work
    assert_command_succeeds "sudo -u postgres psql -c 'SELECT 1;'" "PostgreSQL works"
    assert_command_succeeds "redis-cli ping" "Redis works"
    
    # Cleanup
    rm -f "$multi_config"
}

# Test service with custom port
test_service_custom_port() {
    echo "Testing service with custom port configuration..."
    
    # Use the ports config which has custom PostgreSQL port
    create_test_vm "$CONFIG_DIR/ports.json" || return 1
    
    # Check PostgreSQL is installed
    assert_service_enabled "postgresql" "PostgreSQL should be installed"
    
    # Check custom port is configured
    assert_output_contains "grep port /etc/postgresql/*/main/postgresql.conf | grep -v '#'" "5433" "Custom port configured"
}

# Test all services disabled
test_all_services_disabled() {
    echo "Testing with all services explicitly disabled..."
    
    # Generate config with all services disabled
    local disabled_config="/tmp/all-disabled-$$.json"
    generate_config "all-disabled" '{
        "project": {"name": "test-no-services"},
        "services": {
            "postgresql": {"enabled": false},
            "redis": {"enabled": false},
            "mongodb": {"enabled": false},
            "docker": {"enabled": false}
        }
    }' "$disabled_config"
    
    create_test_vm "$disabled_config" || return 1
    
    # Check all services are NOT installed
    assert_service_not_enabled "postgresql" "PostgreSQL should not be installed"
    assert_service_not_enabled "redis" "Redis should not be installed"
    assert_service_not_enabled "mongodb" "MongoDB should not be installed"
    assert_service_not_enabled "docker" "Docker should not be installed"
    
    # Cleanup
    rm -f "$disabled_config"
}

# Test service configuration persistence
test_service_persistence() {
    echo "Testing service configuration persistence after reload..."
    
    # Create VM with PostgreSQL
    create_test_vm "$CONFIG_DIR/services/postgresql.json" || return 1
    
    # Create a database
    assert_command_succeeds "sudo -u postgres createdb testdb" "Create test database"
    
    # Insert test data
    assert_command_succeeds "sudo -u postgres psql testdb -c 'CREATE TABLE test (id INT);'" "Create table"
    assert_command_succeeds "sudo -u postgres psql testdb -c 'INSERT INTO test VALUES (42);'" "Insert data"
    
    # Halt and restart VM
    cd "$TEST_DIR"
    vm halt || return 1
    sleep 5
    vm up || return 1
    sleep 5
    
    # Check data persists
    assert_output_contains "sudo -u postgres psql testdb -c 'SELECT * FROM test;'" "42" "Data persists after restart"
}