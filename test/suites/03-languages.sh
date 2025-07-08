#!/bin/bash
# Test Suite: Language Package Tests
# Tests installation of language runtimes and packages

# Test npm packages installation
test_npm_packages() {
    echo "Testing npm packages installation..."
    
    create_test_vm "$CONFIG_DIR/languages/npm_packages.json" || return 1
    
    # Check Node.js is available (should be default)
    assert_command_succeeds "node --version" "Node.js is installed"
    assert_command_succeeds "npm --version" "npm is installed"
    
    # Check specific packages are installed
    assert_command_succeeds "which prettier" "Prettier is installed"
    assert_command_succeeds "which eslint" "ESLint is installed"
    
    # Test package functionality
    assert_command_succeeds "prettier --version" "Prettier works"
    assert_command_succeeds "eslint --version" "ESLint works"
    
    # Check they're globally accessible
    assert_output_contains "npm list -g prettier" "prettier" "Prettier installed globally"
    assert_output_contains "npm list -g eslint" "eslint" "ESLint installed globally"
}

# Test cargo packages trigger Rust installation
test_cargo_packages() {
    echo "Testing cargo packages and Rust installation..."
    
    create_test_vm "$CONFIG_DIR/languages/cargo_packages.json" || return 1
    
    # Check Rust toolchain is installed
    assert_command_succeeds "which rustc" "Rust compiler is installed"
    assert_command_succeeds "which cargo" "Cargo is installed"
    assert_command_succeeds "rustc --version" "Rust compiler works"
    
    # Check specific packages are installed
    assert_command_succeeds "which rg" "ripgrep is installed"
    assert_command_succeeds "which tokei" "tokei is installed"
    
    # Test package functionality
    assert_command_succeeds "rg --version" "ripgrep works"
    assert_command_succeeds "tokei --version" "tokei works"
    
    # Check PATH includes cargo bin
    assert_output_contains "echo \$PATH" ".cargo/bin" "Cargo bin in PATH"
}

# Test pip packages trigger Python/pyenv installation
test_pip_packages() {
    echo "Testing pip packages and Python installation..."
    
    create_test_vm "$CONFIG_DIR/languages/pip_packages.json" || return 1
    
    # Check pyenv is installed
    assert_command_succeeds "which pyenv" "pyenv is installed"
    
    # Check Python is available via pyenv
    assert_command_succeeds "python --version" "Python is installed"
    assert_command_succeeds "pip --version" "pip is installed"
    
    # Check specific packages are installed
    assert_command_succeeds "which black" "black is installed"
    assert_command_succeeds "which pytest" "pytest is installed"
    
    # Test package functionality
    assert_command_succeeds "black --version" "black works"
    assert_command_succeeds "pytest --version" "pytest works"
}

# Test empty package arrays don't install runtimes
test_empty_packages() {
    echo "Testing empty package arrays don't trigger installations..."
    
    # Use minimal config (has empty arrays)
    create_test_vm "$CONFIG_DIR/minimal.json" || return 1
    
    # Node.js should still be installed (it's part of base)
    assert_command_succeeds "which node" "Node.js is installed by default"
    
    # But Rust and pyenv should NOT be installed
    assert_command_fails "which cargo" "Cargo should not be installed"
    assert_command_fails "which pyenv" "pyenv should not be installed"
}

# Test multiple language packages together
test_multiple_languages() {
    echo "Testing multiple language packages together..."
    
    # Generate config with all package types
    local multi_lang_config="/tmp/multi-lang-$$.json"
    generate_config "multi-lang" '{
        "project": {"name": "test-multi-lang"},
        "npm_packages": ["prettier"],
        "cargo_packages": ["ripgrep"],
        "pip_packages": ["black"]
    }' "$multi_lang_config"
    
    create_test_vm "$multi_lang_config" || return 1
    
    # Check all runtimes are installed
    assert_command_succeeds "node --version" "Node.js is installed"
    assert_command_succeeds "rustc --version" "Rust is installed"
    assert_command_succeeds "python --version" "Python is installed"
    
    # Check all packages are installed
    assert_command_succeeds "which prettier" "prettier is installed"
    assert_command_succeeds "which rg" "ripgrep is installed"
    assert_command_succeeds "which black" "black is installed"
    
    # Cleanup
    rm -f "$multi_lang_config"
}

# Test package installation with custom versions
test_custom_versions() {
    echo "Testing language installations with custom versions..."
    
    # Generate config with specific versions
    local version_config="/tmp/versions-$$.json"
    generate_config "versions" '{
        "project": {"name": "test-versions"},
        "versions": {
            "node": "20",
            "rust": "stable",
            "python": "3.11"
        },
        "npm_packages": ["prettier"],
        "cargo_packages": ["ripgrep"]
    }' "$version_config"
    
    create_test_vm "$version_config" || return 1
    
    # Check Node version
    assert_output_contains "node --version" "v20" "Node.js v20 is installed"
    
    # Check Rust is stable
    assert_command_succeeds "rustc --version" "Rust stable is installed"
    
    # Cleanup
    rm -f "$version_config"
}

# Test package commands are in PATH
test_package_path_integration() {
    echo "Testing package commands are properly in PATH..."
    
    create_test_vm "$CONFIG_DIR/languages/npm_packages.json" || return 1
    
    # Test that packages work without specifying full path
    assert_command_succeeds "cd /tmp && prettier --version" "Prettier works from any directory"
    assert_command_succeeds "cd /workspace && eslint --version" "ESLint works from workspace"
    
    # Test packages work in new shell session
    assert_command_succeeds "bash -c 'source ~/.zshrc && prettier --version'" "Packages work in new shell"
}