#!/bin/bash
# preset-system.test.sh - Integration test suite for smart preset system
# Tests preset application, preset commands, and system integration

set -euo pipefail

# Script directory and paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VM_TOOL="$SCRIPT_DIR/../../vm.sh"
PROJECT_DETECTOR="$SCRIPT_DIR/../../shared/project-detector.sh"
PRESETS_DIR="$SCRIPT_DIR/../../configs/presets"
TEST_TEMP_DIR="/tmp/vm-preset-tests"

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Test result tracking
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0
FAILED_TESTS=()

# Test helper functions
setup_test_env() {
    echo -e "${BLUE}Setting up test environment...${NC}"
    rm -rf "$TEST_TEMP_DIR"
    mkdir -p "$TEST_TEMP_DIR"

    # Source the project detector for testing
    if [[ -f "$PROJECT_DETECTOR" ]]; then
        source "$PROJECT_DETECTOR"
    else
        echo -e "${RED}ERROR: Project detector not found at $PROJECT_DETECTOR${NC}"
        exit 1
    fi
}

cleanup_test_env() {
    echo -e "${BLUE}Cleaning up test environment...${NC}"
    rm -rf "$TEST_TEMP_DIR"
}

# Test result helpers
pass_test() {
    local test_name="$1"
    echo -e "${GREEN}✓ $test_name${NC}"
    ((TESTS_PASSED++))
}

fail_test() {
    local test_name="$1"
    local error_msg="${2:-No error message provided}"
    echo -e "${RED}✗ $test_name${NC}"
    echo -e "${RED}  Error: $error_msg${NC}"
    FAILED_TESTS+=("$test_name: $error_msg")
    ((TESTS_FAILED++))
}

assert_equals() {
    local expected="$1"
    local actual="$2"
    local test_name="$3"

    ((TESTS_RUN++))

    if [[ "$expected" == "$actual" ]]; then
        pass_test "$test_name"
        return 0
    else
        fail_test "$test_name" "Expected '$expected', got '$actual'"
        return 1
    fi
}

assert_contains() {
    local haystack="$1"
    local needle="$2"
    local test_name="$3"

    ((TESTS_RUN++))

    if [[ "$haystack" == *"$needle"* ]]; then
        pass_test "$test_name"
        return 0
    else
        fail_test "$test_name" "Expected to find '$needle' in output"
        return 1
    fi
}

assert_not_contains() {
    local haystack="$1"
    local needle="$2"
    local test_name="$3"

    ((TESTS_RUN++))

    if [[ "$haystack" != *"$needle"* ]]; then
        pass_test "$test_name"
        return 0
    else
        fail_test "$test_name" "Did not expect to find '$needle' in output"
        return 1
    fi
}

assert_file_exists() {
    local file_path="$1"
    local test_name="$2"

    ((TESTS_RUN++))

    if [[ -f "$file_path" ]]; then
        pass_test "$test_name"
        return 0
    else
        fail_test "$test_name" "File does not exist: $file_path"
        return 1
    fi
}

# Helper function to create test project structures
create_test_project() {
    local project_name="$1"
    local project_type="$2"
    local project_dir="$TEST_TEMP_DIR/$project_name"

    mkdir -p "$project_dir"

    case "$project_type" in
        "react")
            cat > "$project_dir/package.json" <<EOF
{
  "name": "test-react-app",
  "version": "1.0.0",
  "dependencies": {
    "react": "^18.2.0",
    "react-dom": "^18.2.0"
  },
  "devDependencies": {
    "react-scripts": "5.0.1"
  }
}
EOF
            ;;
        "vue")
            cat > "$project_dir/package.json" <<EOF
{
  "name": "test-vue-app",
  "version": "1.0.0",
  "dependencies": {
    "vue": "^3.3.0"
  },
  "devDependencies": {
    "@vitejs/plugin-vue": "^4.0.0"
  }
}
EOF
            ;;
        "next")
            cat > "$project_dir/package.json" <<EOF
{
  "name": "test-nextjs-app",
  "version": "1.0.0",
  "dependencies": {
    "next": "^13.4.0",
    "react": "^18.2.0",
    "react-dom": "^18.2.0"
  }
}
EOF
            ;;
        "angular")
            cat > "$project_dir/package.json" <<EOF
{
  "name": "test-angular-app",
  "version": "1.0.0",
  "dependencies": {
    "@angular/core": "^15.2.0",
    "@angular/common": "^15.2.0",
    "@angular/platform-browser": "^15.2.0"
  }
}
EOF
            ;;
        "django")
            cat > "$project_dir/requirements.txt" <<EOF
Django==4.2.0
psycopg2-binary==2.9.6
python-decouple==3.8
EOF
            ;;
        "flask")
            cat > "$project_dir/requirements.txt" <<EOF
Flask==2.3.0
Flask-SQLAlchemy==3.0.5
python-dotenv==1.0.0
EOF
            ;;
        "rails")
            cat > "$project_dir/Gemfile" <<EOF
source 'https://rubygems.org'
git_source(:github) { |repo| "https://github.com/#{repo}.git" }

ruby '3.0.0'

gem 'rails', '~> 7.0.0'
gem 'pg', '~> 1.1'
gem 'puma', '~> 5.0'
EOF
            ;;
        "nodejs")
            cat > "$project_dir/package.json" <<EOF
{
  "name": "test-nodejs-app",
  "version": "1.0.0",
  "dependencies": {
    "express": "^4.18.0",
    "lodash": "^4.17.21"
  }
}
EOF
            ;;
        "python")
            cat > "$project_dir/requirements.txt" <<EOF
requests==2.31.0
numpy==1.24.0
pandas==2.0.0
EOF
            ;;
        "docker")
            cat > "$project_dir/Dockerfile" <<EOF
FROM node:18-alpine
WORKDIR /app
COPY package*.json ./
RUN npm install
COPY . .
EXPOSE 3000
CMD ["npm", "start"]
EOF
            cat > "$project_dir/docker-compose.yml" <<EOF
version: '3.8'
services:
  app:
    build: .
    ports:
      - "3000:3000"
EOF
            ;;
        "kubernetes")
            mkdir -p "$project_dir/k8s"
            cat > "$project_dir/k8s/deployment.yaml" <<EOF
apiVersion: apps/v1
kind: Deployment
metadata:
  name: test-app
spec:
  replicas: 3
  selector:
    matchLabels:
      app: test-app
  template:
    metadata:
      labels:
        app: test-app
    spec:
      containers:
      - name: test-app
        image: test-app:latest
        ports:
        - containerPort: 3000
EOF
            ;;
        "rust")
            cat > "$project_dir/Cargo.toml" <<EOF
[package]
name = "test-rust-app"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = "1.0"
serde = "1.0"
EOF
            ;;
        "go")
            cat > "$project_dir/go.mod" <<EOF
module test-go-app

go 1.20

require (
    github.com/gin-gonic/gin v1.9.1
    github.com/gorilla/mux v1.8.0
)
EOF
            ;;
        "multi-react-django")
            # Create React frontend
            cat > "$project_dir/package.json" <<EOF
{
  "name": "fullstack-app-frontend",
  "version": "1.0.0",
  "dependencies": {
    "react": "^18.2.0",
    "react-dom": "^18.2.0"
  }
}
EOF
            # Create Django backend
            cat > "$project_dir/requirements.txt" <<EOF
Django==4.2.0
djangorestframework==3.14.0
EOF
            ;;
        "multi-docker-react")
            # Create React app
            cat > "$project_dir/package.json" <<EOF
{
  "name": "dockerized-react-app",
  "version": "1.0.0",
  "dependencies": {
    "react": "^18.2.0",
    "react-dom": "^18.2.0"
   }
}
EOF
            # Create Docker files
            cat > "$project_dir/Dockerfile" <<EOF
FROM node:18-alpine
WORKDIR /app
COPY package*.json ./
RUN npm install
COPY . .
EXPOSE 3000
CMD ["npm", "start"]
EOF
            ;;
        "empty")
            # Empty directory for generic detection
            ;;
        "malformed-json")
            cat > "$project_dir/package.json" <<EOF
{
  "name": "broken-app",
  "version": "1.0.0"
  "dependencies": {
    "react": "^18.2.0"
  // Missing closing brace and comma
EOF
            ;;
    esac

    echo "$project_dir"
}

# NOTE: Framework detection tests have been moved to test/unit/preset-detection.test.sh
# This file focuses on preset application and system integration tests

# Test preset application functionality
test_preset_application() {
    echo -e "\n${PURPLE}=== Preset Application Tests ===${NC}"

    # Test React preset dry-run
    local react_dir
    react_dir=$(create_test_project "preset-react-test" "react")
    cd "$react_dir"

    local dry_run_output
    dry_run_output=$("$VM_TOOL" --dry-run --no-preset create 2>&1 || true)
    assert_contains "$dry_run_output" "Dry run mode" "React preset dry-run execution"

    # Test forced preset override
    local forced_output
    forced_output=$("$VM_TOOL" --dry-run --preset django create 2>&1 || true)
    assert_contains "$forced_output" "forced preset: django" "Forced preset override"

    # Test no-preset flag
    local no_preset_output
    no_preset_output=$("$VM_TOOL" --dry-run --no-preset create 2>&1 || true)
    assert_not_contains "$no_preset_output" "Detecting project type" "No-preset flag disables detection"

    cd "$SCRIPT_DIR"
}

# Test vm preset commands
test_preset_commands() {
    echo -e "\n${PURPLE}=== Preset Commands Tests ===${NC}"

    # Test preset list command
    local list_output
    list_output=$("$VM_TOOL" preset list 2>&1 || true)
    assert_contains "$list_output" "Available VM Presets" "Preset list command header"
    assert_contains "$list_output" "react" "Preset list contains react"
    assert_contains "$list_output" "django" "Preset list contains django"
    assert_contains "$list_output" "base" "Preset list contains base"

    # Test preset show command for react
    local show_react_output
    show_react_output=$("$VM_TOOL" preset show react 2>&1 || true)
    assert_contains "$show_react_output" "React Development" "Preset show react contains title"
    assert_contains "$show_react_output" "npm_packages" "Preset show react contains npm packages"
    assert_contains "$show_react_output" "ports" "Preset show react contains ports"

    # Test preset show command for django
    local show_django_output
    show_django_output=$("$VM_TOOL" preset show django 2>&1 || true)
    assert_contains "$show_django_output" "Django" "Preset show django contains Django"
    assert_contains "$show_django_output" "pip_packages" "Preset show django contains pip packages"

    # Test preset show with .yaml extension
    local show_yaml_output
    show_yaml_output=$("$VM_TOOL" preset show react.yaml 2>&1 || true)
    assert_contains "$show_yaml_output" "React Development" "Preset show with .yaml extension works"

    # Test preset show with non-existent preset
    local show_missing_output
    show_missing_output=$("$VM_TOOL" preset show nonexistent 2>&1 || true)
    assert_contains "$show_missing_output" "not found" "Preset show handles missing preset"

    # Test preset command without subcommand
    local no_subcommand_output
    no_subcommand_output=$("$VM_TOOL" preset 2>&1 || true)
    assert_contains "$no_subcommand_output" "Missing preset subcommand" "Preset command without subcommand shows error"
    assert_contains "$no_subcommand_output" "list" "Preset help shows list subcommand"
    assert_contains "$no_subcommand_output" "show" "Preset help shows show subcommand"

    # Test preset command with invalid subcommand
    local invalid_subcommand_output
    invalid_subcommand_output=$("$VM_TOOL" preset invalid 2>&1 || true)
    assert_contains "$invalid_subcommand_output" "Unknown preset subcommand" "Invalid preset subcommand shows error"
}

# Test flag functionality
test_flag_functionality() {
    echo -e "\n${PURPLE}=== Flag Functionality Tests ===${NC}"

    # Create test project
    local test_dir
    test_dir=$(create_test_project "flag-test" "react")
    cd "$test_dir"

    # Test --no-preset flag
    local no_preset_output
    no_preset_output=$("$VM_TOOL" --dry-run --no-preset create 2>&1 || true)
    assert_not_contains "$no_preset_output" "Detecting project type" "--no-preset disables detection"
    assert_not_contains "$no_preset_output" "Applying.*preset" "--no-preset disables preset application"

    # Test --preset NAME flag
    local forced_preset_output
    forced_preset_output=$("$VM_TOOL" --dry-run --preset python create 2>&1 || true)
    assert_contains "$forced_preset_output" "forced preset: python" "--preset forces specific preset"

    # Test multiple flags together (should prioritize --no-preset)
    local conflicting_flags_output
    conflicting_flags_output=$("$VM_TOOL" --dry-run --no-preset --preset django create 2>&1 || true)
    assert_not_contains "$conflicting_flags_output" "Detecting project type" "Conflicting flags: --no-preset takes priority"

    cd "$SCRIPT_DIR"
}

# Test edge cases and error handling
test_edge_cases() {
    echo -e "\n${PURPLE}=== Edge Cases Tests ===${NC}"

    # Test malformed package.json handling in preset application
    local malformed_dir
    malformed_dir=$(create_test_project "malformed-json" "malformed-json")

    # Preset application should handle malformed JSON gracefully
    local detected_type
    detected_type=$(detect_project_type "$malformed_dir" 2>/dev/null || echo "generic")
    assert_equals "generic" "$detected_type" "Malformed JSON handled gracefully in preset context"

    # Test missing presets directory (simulate by temporarily moving it)
    if [[ -d "$PRESETS_DIR" ]]; then
        mv "$PRESETS_DIR" "${PRESETS_DIR}.backup"
        local missing_presets_output
        missing_presets_output=$("$VM_TOOL" preset list 2>&1 || true)
        assert_contains "$missing_presets_output" "not found" "Missing presets directory handled"
        mv "${PRESETS_DIR}.backup" "$PRESETS_DIR"
    fi

    # Test detection with unreadable directory
    local unreadable_dir="$TEST_TEMP_DIR/unreadable"
    mkdir -p "$unreadable_dir"
    chmod 000 "$unreadable_dir" 2>/dev/null || true

    local unreadable_type
    unreadable_type=$(detect_project_type "$unreadable_dir" 2>/dev/null || echo "error")
    chmod 755 "$unreadable_dir" 2>/dev/null || true

    # Should handle permission errors gracefully
    ((TESTS_RUN++))
    if [[ "$unreadable_type" == "error" || "$unreadable_type" == "generic" ]]; then
        pass_test "Unreadable directory handled gracefully"
    else
        fail_test "Unreadable directory handling" "Expected 'error' or 'generic', got '$unreadable_type'"
    fi

    # Test project detection with non-existent directory
    local nonexistent_type
    nonexistent_type=$(detect_project_type "/path/that/does/not/exist" 2>/dev/null || echo "error")
    ((TESTS_RUN++))
    if [[ "$nonexistent_type" == "error" ]]; then
        pass_test "Non-existent directory handled"
    else
        fail_test "Non-existent directory handling" "Expected 'error', got '$nonexistent_type'"
    fi
}

# Test preset file validation
test_preset_files() {
    echo -e "\n${PURPLE}=== Preset Files Validation ===${NC}"

    # Check that all expected preset files exist
    local expected_presets=("base" "react" "vue" "django" "flask" "rails" "nodejs" "python" "docker" "kubernetes")

    for preset in "${expected_presets[@]}"; do
        assert_file_exists "$PRESETS_DIR/${preset}.yaml" "Preset file exists: $preset"
    done

    # Validate preset file structure (basic YAML syntax)
    for preset_file in "$PRESETS_DIR"/*.yaml; do
        if [[ -f "$preset_file" ]]; then
            local preset_name
            preset_name=$(basename "$preset_file" .yaml)

            # Check if file is readable and contains expected sections
            ((TESTS_RUN++))
            if grep -q "preset:" "$preset_file" 2>/dev/null; then
                pass_test "Preset file structure valid: $preset_name"
            else
                fail_test "Preset file structure invalid: $preset_name" "Missing 'preset:' section"
            fi
        fi
    done
}

# Test project info functionality
test_project_info() {
    echo -e "\n${PURPLE}=== Project Info Tests ===${NC}"

    # Test project info for React project
    local react_dir
    react_dir=$(create_test_project "info-react" "react")
    local react_info
    react_info=$(get_project_info "$react_dir")
    assert_contains "$react_info" "Project Type: react" "Project info shows React type"
    assert_contains "$react_info" "Package Manager: npm" "Project info shows npm as package manager"

    # Test project info for Django project
    local django_dir
    django_dir=$(create_test_project "info-django" "django")
    local django_info
    django_info=$(get_project_info "$django_dir")
    assert_contains "$django_info" "Project Type: django" "Project info shows Django type"
    assert_contains "$django_info" "Framework: django" "Project info shows Django framework"

    # Test project info for multi-tech project
    local multi_dir
    multi_dir=$(create_test_project "info-multi" "multi-react-django")
    local multi_info
    multi_info=$(get_project_info "$multi_dir")
    assert_contains "$multi_info" "Multi-language project" "Project info shows multi-language"

    # Test version control detection
    local git_dir
    git_dir=$(create_test_project "info-git" "react")
    cd "$git_dir"
    git init --quiet 2>/dev/null || true
    local git_info
    git_info=$(get_project_info "$git_dir")
    assert_contains "$git_info" "Version Control: Git" "Project info detects Git"
    cd "$SCRIPT_DIR"
}

# VM resource suggestions functionality moved to Rust implementation
# See: rust/vm-config/src/resources.rs for comprehensive tests

# Main test runner for integration tests
run_all_tests() {
    echo -e "${CYAN}Preset System Integration Test Suite${NC}"
    echo -e "${CYAN}=====================================${NC}"

    setup_test_env

    # Set up trap for cleanup
    trap cleanup_test_env EXIT

    # Run integration test suites (framework detection moved to unit tests)
    test_preset_application
    test_preset_commands
    test_flag_functionality
    test_edge_cases
    test_preset_files
    test_project_info
    test_vm_resources

    # Print summary
    echo -e "\n${CYAN}=== Test Summary ===${NC}"
    echo -e "Tests run: $TESTS_RUN"
    echo -e "${GREEN}Passed: $TESTS_PASSED${NC}"
    echo -e "${RED}Failed: $TESTS_FAILED${NC}"

    if [[ $TESTS_FAILED -gt 0 ]]; then
        echo -e "\n${RED}Failed tests:${NC}"
        for failed_test in "${FAILED_TESTS[@]}"; do
            echo -e "${RED}  • $failed_test${NC}"
        done
        echo ""
        exit 1
    else
        echo -e "\n${GREEN}All tests passed! 🎉${NC}"
        exit 0
    fi
}

# Help function
show_help() {
    echo "Preset System Integration Test Suite"
    echo ""
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --help, -h          Show this help message"
    echo "  --application       Run only preset application tests"
    echo "  --commands          Run only preset command tests"
    echo "  --flags             Run only flag functionality tests"
    echo "  --edge-cases        Run only edge case tests"
    echo "  --preset-files      Run only preset file validation tests"
    echo "  --project-info      Run only project info tests"
    echo "  --vm-resources      Run only VM resource suggestion tests"
    echo ""
    echo "Note: Framework detection tests are now in test/unit/preset-detection.test.sh"
    echo ""
    echo "Examples:"
    echo "  $0                  Run all integration tests"
    echo "  $0 --application    Run only preset application tests"
    echo "  $0 --commands       Run only preset command tests"
}

# Parse command line arguments
case "${1:-}" in
    --help|-h)
        show_help
        exit 0
        ;;
    # Framework detection tests moved to unit tests
    --framework)
        echo -e "${YELLOW}Framework detection tests have been moved to test/unit/preset-detection.test.sh${NC}"
        echo -e "${YELLOW}Run: ./test/unit/preset-detection.test.sh --detection${NC}"
        exit 0
        ;;
    --application)
        setup_test_env
        trap cleanup_test_env EXIT
        test_preset_application
        ;;
    --commands)
        test_preset_commands
        ;;
    --flags)
        setup_test_env
        trap cleanup_test_env EXIT
        test_flag_functionality
        ;;
    --edge-cases)
        setup_test_env
        trap cleanup_test_env EXIT
        test_edge_cases
        ;;
    --preset-files)
        test_preset_files
        ;;
    --project-info)
        setup_test_env
        trap cleanup_test_env EXIT
        test_project_info
        ;;
    --vm-resources)
        test_vm_resources
        ;;
    "")
        run_all_tests
        ;;
    *)
        echo "Unknown option: $1"
        show_help
        exit 1
        ;;
esac

# Print individual test summary if not running all tests
if [[ "${1:-}" != "" ]]; then
    echo -e "\n${CYAN}Test Summary${NC}"
    echo -e "Tests run: $TESTS_RUN"
    echo -e "${GREEN}Passed: $TESTS_PASSED${NC}"
    echo -e "${RED}Failed: $TESTS_FAILED${NC}"

    if [[ $TESTS_FAILED -gt 0 ]]; then
        echo -e "\n${RED}Failed tests:${NC}"
        for failed_test in "${FAILED_TESTS[@]}"; do
            echo -e "${RED}  • $failed_test${NC}"
        done
        exit 1
    else
        echo -e "\n${GREEN}Selected tests passed!${NC}"
        exit 0
    fi
fi