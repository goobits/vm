#!/bin/bash
# Unit tests for framework detection functionality
# Extracted from comprehensive preset test suite to focus on detection logic only

set -euo pipefail

# Script directory and paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DETECTOR="$SCRIPT_DIR/../../shared/project-detector.sh"
TEST_TEMP_DIR="/tmp/vm-preset-detection-tests"

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

# Framework Detection Tests - Core unit test functionality
test_framework_detection() {
    echo -e "\n${PURPLE}=== Framework Detection Tests ===${NC}"

    # Test React detection
    local react_dir
    react_dir=$(create_test_project "react-project" "react")
    local detected_type
    detected_type=$(detect_project_type "$react_dir")
    assert_equals "react" "$detected_type" "React project detection"

    # Test Vue detection
    local vue_dir
    vue_dir=$(create_test_project "vue-project" "vue")
    detected_type=$(detect_project_type "$vue_dir")
    assert_equals "vue" "$detected_type" "Vue project detection"

    # Test Next.js detection (should override React)
    local next_dir
    next_dir=$(create_test_project "next-project" "next")
    detected_type=$(detect_project_type "$next_dir")
    assert_equals "next" "$detected_type" "Next.js project detection"

    # Test Angular detection
    local angular_dir
    angular_dir=$(create_test_project "angular-project" "angular")
    detected_type=$(detect_project_type "$angular_dir")
    assert_equals "angular" "$detected_type" "Angular project detection"

    # Test Django detection
    local django_dir
    django_dir=$(create_test_project "django-project" "django")
    detected_type=$(detect_project_type "$django_dir")
    assert_equals "django" "$detected_type" "Django project detection"

    # Test Flask detection
    local flask_dir
    flask_dir=$(create_test_project "flask-project" "flask")
    detected_type=$(detect_project_type "$flask_dir")
    assert_equals "flask" "$detected_type" "Flask project detection"

    # Test Rails detection
    local rails_dir
    rails_dir=$(create_test_project "rails-project" "rails")
    detected_type=$(detect_project_type "$rails_dir")
    assert_equals "rails" "$detected_type" "Rails project detection"

    # Test generic Node.js detection
    local nodejs_dir
    nodejs_dir=$(create_test_project "nodejs-project" "nodejs")
    detected_type=$(detect_project_type "$nodejs_dir")
    assert_equals "nodejs" "$detected_type" "Node.js project detection"

    # Test Python detection
    local python_dir
    python_dir=$(create_test_project "python-project" "python")
    detected_type=$(detect_project_type "$python_dir")
    assert_equals "python" "$detected_type" "Python project detection"

    # Test Docker detection
    local docker_dir
    docker_dir=$(create_test_project "docker-project" "docker")
    detected_type=$(detect_project_type "$docker_dir")
    assert_equals "docker" "$detected_type" "Docker project detection"

    # Test Kubernetes detection
    local k8s_dir
    k8s_dir=$(create_test_project "k8s-project" "kubernetes")
    detected_type=$(detect_project_type "$k8s_dir")
    assert_equals "kubernetes" "$detected_type" "Kubernetes project detection"

    # Test Rust detection
    local rust_dir
    rust_dir=$(create_test_project "rust-project" "rust")
    detected_type=$(detect_project_type "$rust_dir")
    assert_equals "rust" "$detected_type" "Rust project detection"

    # Test Go detection
    local go_dir
    go_dir=$(create_test_project "go-project" "go")
    detected_type=$(detect_project_type "$go_dir")
    assert_equals "go" "$detected_type" "Go project detection"

    # Test multi-technology project
    local multi_dir
    multi_dir=$(create_test_project "multi-project" "multi-react-django")
    detected_type=$(detect_project_type "$multi_dir")
    assert_equals "multi:react django" "$detected_type" "Multi-tech project detection (React + Django)"

    # Test multi-tech with Docker
    local multi_docker_dir
    multi_docker_dir=$(create_test_project "multi-docker-project" "multi-docker-react")
    detected_type=$(detect_project_type "$multi_docker_dir")
    assert_equals "multi:react docker" "$detected_type" "Multi-tech project detection (React + Docker)"

    # Test empty directory (generic)
    local empty_dir
    empty_dir=$(create_test_project "empty-project" "empty")
    detected_type=$(detect_project_type "$empty_dir")
    assert_equals "generic" "$detected_type" "Empty directory detection (generic)"
}

# Test edge cases in detection
test_detection_edge_cases() {
    echo -e "\n${PURPLE}=== Detection Edge Cases Tests ===${NC}"

    # Test malformed package.json
    local malformed_dir
    malformed_dir=$(create_test_project "malformed-json" "malformed-json")

    # Detection should handle malformed JSON gracefully
    local detected_type
    detected_type=$(detect_project_type "$malformed_dir" 2>/dev/null || echo "generic")
    assert_equals "generic" "$detected_type" "Malformed JSON handled gracefully"

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

# Main test runner for detection-focused tests
run_detection_tests() {
    echo -e "${CYAN}Framework Detection Unit Tests${NC}"
    echo -e "${CYAN}==============================${NC}"

    setup_test_env

    # Set up trap for cleanup
    trap cleanup_test_env EXIT

    # Run detection-focused test suites
    test_framework_detection
    test_detection_edge_cases

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
        echo -e "\n${GREEN}All detection tests passed! 🎉${NC}"
        exit 0
    fi
}

# Help function
show_help() {
    echo "Framework Detection Unit Test Suite"
    echo ""
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --help, -h          Show this help message"
    echo "  --detection         Run framework detection tests"
    echo "  --edge-cases        Run edge case tests"
    echo ""
    echo "Examples:"
    echo "  $0                  Run all detection tests"
    echo "  $0 --detection      Run only framework detection tests"
    echo "  $0 --edge-cases     Run only edge case tests"
}

# Parse command line arguments
case "${1:-}" in
    --help|-h)
        show_help
        exit 0
        ;;
    --detection)
        setup_test_env
        trap cleanup_test_env EXIT
        test_framework_detection
        ;;
    --edge-cases)
        setup_test_env
        trap cleanup_test_env EXIT
        test_detection_edge_cases
        ;;
    "")
        run_detection_tests
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