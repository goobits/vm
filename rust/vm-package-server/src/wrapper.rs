use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
// Health check and timing functionality is handled by reqwest and file metadata
use tracing::debug;

/// Generate shell functions for the current or specified shell
pub fn generate_shell_functions(shell: Option<String>, port: u16, ttl: u64) -> Result<()> {
    let detected_shell = detect_shell(shell)?;

    match detected_shell.as_str() {
        "bash" | "zsh" => generate_bash_zsh_functions(port, ttl),
        "fish" => generate_fish_functions(port, ttl),
        "pwsh" | "powershell" => generate_powershell_functions(port, ttl),
        _ => {
            eprintln!(
                "Warning: Unknown shell '{}', generating bash-compatible output",
                detected_shell
            );
            generate_bash_zsh_functions(port, ttl)
        }
    }
}

/// Execute a command with health-checked wrapper
pub async fn exec_with_wrapper(command: &str, args: &[String]) -> Result<i32> {
    let port = env::var("PKG_SERVER_PORT")
        .unwrap_or_else(|_| "3080".to_string())
        .parse::<u16>()
        .unwrap_or(3080);

    // Check if server is healthy
    let is_healthy = check_server_health(port).await;

    // Find the actual command binary
    let real_command =
        which::which(command).with_context(|| format!("Command '{}' not found", command))?;

    let mut cmd = Command::new(&real_command);

    // Add registry flags if server is healthy and command supports them
    if is_healthy {
        match command {
            "npm" | "pnpm" => {
                // Only add registry for install/add/publish commands
                if args.is_empty() || should_add_npm_registry(&args[0]) {
                    cmd.arg(format!("--registry=http://localhost:{}/npm/", port));
                }
            }
            "yarn" => {
                if args.is_empty() || should_add_yarn_registry(&args[0]) {
                    cmd.args(["--registry", &format!("http://localhost:{}/npm/", port)]);
                }
            }
            "pip" | "pip3" => {
                // Only add index-url for install/download commands
                if args.is_empty() || should_add_pip_index(&args[0]) {
                    cmd.arg(format!("--index-url=http://localhost:{}/pypi/simple", port));
                    cmd.arg(format!("--trusted-host=localhost:{}", port));
                }
            }
            _ => {} // Pass through unchanged
        }
    }

    // Add original arguments
    cmd.args(args);

    // Execute the command
    let status = cmd
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to execute {}", command))?;

    Ok(status.code().unwrap_or(1))
}

/// Check if the server is healthy (with caching)
async fn check_server_health(port: u16) -> bool {
    let cache_file = format!("/tmp/.pkg-server-health-{}", port);
    let cache_path = Path::new(&cache_file);
    let ttl = env::var("PKG_SERVER_TTL")
        .unwrap_or_else(|_| "5".to_string())
        .parse::<u64>()
        .unwrap_or(5);

    // Check cache
    if let Ok(metadata) = fs::metadata(cache_path) {
        if let Ok(modified) = metadata.modified() {
            if let Ok(elapsed) = modified.elapsed() {
                if elapsed.as_secs() < ttl {
                    debug!("Using cached health status (age: {}s)", elapsed.as_secs());
                    return true;
                }
            }
        }
    }

    // Perform health check
    let url = format!("http://localhost:{}/api/status", port);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(100))
        .build()
        .unwrap();

    match client.get(&url).send().await {
        Ok(response) if response.status().is_success() => {
            // Update cache file
            fs::write(cache_path, "").ok();
            true
        }
        _ => {
            // Remove stale cache
            fs::remove_file(cache_path).ok();
            false
        }
    }
}

/// Detect the shell to generate functions for
fn detect_shell(specified: Option<String>) -> Result<String> {
    if let Some(shell) = specified {
        return Ok(shell);
    }

    if let Ok(shell_env) = env::var("SHELL") {
        let shell_name = shell_env.split('/').next_back().unwrap_or("bash");
        return Ok(shell_name.to_string());
    }

    Ok("bash".to_string())
}

/// Generate bash/zsh compatible functions
fn generate_bash_zsh_functions(port: u16, ttl: u64) -> Result<()> {
    println!(
        r#"# pkg-server shell integration
# Add to ~/.bashrc or ~/.zshrc: eval "$(pkg-server use)"

export PKG_SERVER_PORT={}
export PKG_SERVER_TTL={}

_pkg_srv() {{
    local port="${{PKG_SERVER_PORT:-3080}}"
    local ttl="${{PKG_SERVER_TTL:-5}}"
    local hf="/tmp/.pkg-server-health-$port"

    # Check cache freshness
    if [ -f "$hf" ]; then
        local age=$(( $(date +%s) - $(stat -c %Y "$hf" 2>/dev/null || stat -f %m "$hf" 2>/dev/null || echo 0) ))
        [ "$age" -lt "$ttl" ] && return 0
    fi

    # Health check with timeout
    if curl -sf -m 0.1 "http://localhost:${{port}}/api/status" >/dev/null 2>&1; then
        touch "$hf"
        return 0
    else
        rm -f "$hf" 2>/dev/null
        return 1
    fi
}}

_pkg_srv_cmd() {{
    local tool="$1"; shift
    local real_tool

    # Find the real tool (not our function)
    if command -v /usr/bin/$tool >/dev/null 2>&1; then
        real_tool="/usr/bin/$tool"
    elif command -v /usr/local/bin/$tool >/dev/null 2>&1; then
        real_tool="/usr/local/bin/$tool"
    elif command -v /opt/homebrew/bin/$tool >/dev/null 2>&1; then
        real_tool="/opt/homebrew/bin/$tool"
    else
        real_tool="$(which -a $tool 2>/dev/null | grep -v "pkg_srv" | head -1)"
    fi

    if [ -z "$real_tool" ]; then
        echo "pkg-server: $tool not found" >&2
        return 127
    fi

    if _pkg_srv; then
        case "$tool" in
            npm|pnpm)
                "$real_tool" --registry="http://localhost:${{PKG_SERVER_PORT:-3080}}/npm/" "$@"
                ;;
            yarn)
                "$real_tool" --registry "http://localhost:${{PKG_SERVER_PORT:-3080}}/npm/" "$@"
                ;;
            pip|pip3)
                "$real_tool" --index-url="http://localhost:${{PKG_SERVER_PORT:-3080}}/pypi/simple" \
                             --trusted-host="localhost:${{PKG_SERVER_PORT:-3080}}" "$@"
                ;;
            *)
                "$real_tool" "$@"
                ;;
        esac
    else
        "$real_tool" "$@"
    fi
}}

# Define wrapper functions
npm()  {{ _pkg_srv_cmd npm "$@"; }}
pnpm() {{ _pkg_srv_cmd pnpm "$@"; }}
yarn() {{ _pkg_srv_cmd yarn "$@"; }}
pip()  {{ _pkg_srv_cmd pip "$@"; }}
pip3() {{ _pkg_srv_cmd pip3 "$@"; }}

# Export functions for subshells
export -f npm pnpm yarn pip pip3 _pkg_srv _pkg_srv_cmd 2>/dev/null || true
"#,
        port, ttl
    );

    Ok(())
}

/// Generate fish shell functions
fn generate_fish_functions(port: u16, ttl: u64) -> Result<()> {
    println!(
        r#"# pkg-server fish shell integration
# Add to ~/.config/fish/config.fish: pkg-server use --shell fish | source

set -gx PKG_SERVER_PORT {}
set -gx PKG_SERVER_TTL {}

function _pkg_srv
    set -l port $PKG_SERVER_PORT
    test -z "$port"; and set port 3080
    set -l ttl $PKG_SERVER_TTL
    test -z "$ttl"; and set ttl 5
    set -l hf "/tmp/.pkg-server-health-$port"

    # Check cache
    if test -f "$hf"
        set -l age (math (date +%s) - (stat -c %Y "$hf" 2>/dev/null; or echo 0))
        test "$age" -lt "$ttl"; and return 0
    end

    # Health check
    if curl -sf -m 0.1 "http://localhost:$port/api/status" >/dev/null 2>&1
        touch "$hf"
        return 0
    else
        rm -f "$hf" 2>/dev/null
        return 1
    end
end

function _pkg_srv_cmd
    set -l tool $argv[1]
    set -e argv[1]

    # Find real tool
    set -l real_tool (which -a $tool 2>/dev/null | grep -v "pkg_srv" | head -1)

    if test -z "$real_tool"
        echo "pkg-server: $tool not found" >&2
        return 127
    end

    if _pkg_srv
        switch $tool
            case npm pnpm
                command $real_tool --registry="http://localhost:$PKG_SERVER_PORT/npm/" $argv
            case yarn
                command $real_tool --registry "http://localhost:$PKG_SERVER_PORT/npm/" $argv
            case pip pip3
                command $real_tool --index-url="http://localhost:$PKG_SERVER_PORT/pypi/simple" \
                                  --trusted-host="localhost:$PKG_SERVER_PORT" $argv
            case '*'
                command $real_tool $argv
        end
    else
        command $real_tool $argv
    end
end

function npm;  _pkg_srv_cmd npm $argv; end
function pnpm; _pkg_srv_cmd pnpm $argv; end
function yarn; _pkg_srv_cmd yarn $argv; end
function pip;  _pkg_srv_cmd pip $argv; end
function pip3; _pkg_srv_cmd pip3 $argv; end
"#,
        port, ttl
    );

    Ok(())
}

/// Generate PowerShell functions
fn generate_powershell_functions(port: u16, ttl: u64) -> Result<()> {
    println!(
        r#"# pkg-server PowerShell integration
# Add to $PROFILE: pkg-server use --shell pwsh | Invoke-Expression

$env:PKG_SERVER_PORT = "{}"
$env:PKG_SERVER_TTL = "{}"

function Test-PkgServer {{
    $port = if ($env:PKG_SERVER_PORT) {{ $env:PKG_SERVER_PORT }} else {{ "3080" }}
    $ttl = if ($env:PKG_SERVER_TTL) {{ $env:PKG_SERVER_TTL }} else {{ 5 }}
    $hf = "$env:TEMP\.pkg-server-health-$port"

    # Check cache
    if (Test-Path $hf) {{
        $age = (Get-Date) - (Get-Item $hf).LastWriteTime
        if ($age.TotalSeconds -lt $ttl) {{ return $true }}
    }}

    # Health check
    try {{
        $null = Invoke-RestMethod -Uri "http://localhost:$port/api/status" -TimeoutSec 0.1
        $null = New-Item -ItemType File -Path $hf -Force
        return $true
    }} catch {{
        Remove-Item -Path $hf -ErrorAction SilentlyContinue
        return $false
    }}
}}

function Invoke-PkgServerCmd {{
    param($Tool, [string[]]$Arguments)

    $realTool = Get-Command -Name $Tool -CommandType Application -ErrorAction SilentlyContinue |
                Select-Object -First 1 -ExpandProperty Source

    if (-not $realTool) {{
        Write-Error "pkg-server: $Tool not found"
        return
    }}

    $port = if ($env:PKG_SERVER_PORT) {{ $env:PKG_SERVER_PORT }} else {{ "3080" }}

    if (Test-PkgServer) {{
        switch ($Tool) {{
            {{$_ -in 'npm','pnpm'}} {{
                & $realTool --registry="http://localhost:$port/npm/" @Arguments
            }}
            'yarn' {{
                & $realTool --registry "http://localhost:$port/npm/" @Arguments
            }}
            {{$_ -in 'pip','pip3'}} {{
                & $realTool --index-url="http://localhost:$port/pypi/simple" `
                           --trusted-host="localhost:$port" @Arguments
            }}
            default {{
                & $realTool @Arguments
            }}
        }}
    }} else {{
        & $realTool @Arguments
    }}
}}

function npm  {{ Invoke-PkgServerCmd -Tool 'npm' -Arguments $args }}
function pnpm {{ Invoke-PkgServerCmd -Tool 'pnpm' -Arguments $args }}
function yarn {{ Invoke-PkgServerCmd -Tool 'yarn' -Arguments $args }}
function pip  {{ Invoke-PkgServerCmd -Tool 'pip' -Arguments $args }}
function pip3 {{ Invoke-PkgServerCmd -Tool 'pip3' -Arguments $args }}
"#,
        port, ttl
    );

    Ok(())
}
/// Check if npm/pnpm command should use registry flag
fn should_add_npm_registry(subcommand: &str) -> bool {
    matches!(
        subcommand,
        "install" | "i" | "add" | "publish" | "pack" | "view" | "info" | "search"
    )
}

/// Check if yarn command should use registry flag
fn should_add_yarn_registry(subcommand: &str) -> bool {
    matches!(
        subcommand,
        "install" | "add" | "publish" | "pack" | "info" | "search"
    )
}

/// Check if pip command should use index-url flag
fn should_add_pip_index(subcommand: &str) -> bool {
    matches!(subcommand, "install" | "download" | "index" | "search")
}
