//! Demo showing the clean platform abstraction API

use vm_platform::platform;

fn main() -> anyhow::Result<()> {
    println!("=== VM Platform Demo ===");

    // Get platform information
    let current_platform = vm_platform::current();
    println!("Platform: {}", current_platform.name());

    // Use convenient functions
    println!("Home directory: {}", platform::home_dir()?.display());
    println!(
        "Config directory: {}",
        platform::user_config_dir()?.display()
    );
    println!("Bin directory: {}", platform::user_bin_dir()?.display());

    // Executable naming
    println!(
        "Executable 'vm' would be named: '{}'",
        platform::executable_name("vm")
    );

    // Shell detection
    let shell = platform::detect_shell()?;
    println!("Detected shell: {}", shell.name());
    if let Some(profile) = shell.profile_path() {
        println!("Shell profile: {}", profile.display());
    }

    // System information
    println!("CPU cores: {}", platform::cpu_core_count()?);
    println!("Memory: {} GB", platform::total_memory_gb()?);

    // Platform-specific package paths
    println!("Cargo home: {}", current_platform.cargo_home()?.display());

    if let Ok(Some(npm_dir)) = current_platform.npm_global_dir() {
        println!("NPM global: {}", npm_dir.display());
    }

    Ok(())
}
