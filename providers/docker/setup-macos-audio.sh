#!/bin/bash
# macOS Audio Setup for Docker Containers
# Configures PulseAudio for network-based audio streaming

set -e

echo "🎵 macOS Docker Audio Setup"
echo "============================"
echo ""

# Check if on macOS
if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "ℹ️  This script is for macOS only"
    echo "   Your system: $(uname -s)"
    exit 0
fi

# Function to check if PulseAudio is running
check_pulseaudio_running() {
    if pgrep -x "pulseaudio" > /dev/null; then
        return 0
    else
        return 1
    fi
}

# Function to install PulseAudio
install_pulseaudio() {
    echo "📦 Installing PulseAudio via Homebrew..."
    
    # Check if Homebrew is installed
    if ! command -v brew >/dev/null 2>&1; then
        echo "❌ Homebrew is not installed"
        echo "   Install from: https://brew.sh"
        exit 1
    fi
    
    # Install PulseAudio
    brew install pulseaudio
    
    echo "✅ PulseAudio installed successfully"
}

# Function to configure PulseAudio for Docker
configure_pulseaudio() {
    local config_dir="$HOME/.config/pulse"
    
    echo "🔧 Configuring PulseAudio for Docker..."
    
    # Create config directory
    mkdir -p "$config_dir"
    
    # Create default.pa configuration
    cat > "$config_dir/default.pa" << 'EOF'
# PulseAudio configuration for Docker containers

# Load system defaults
.include /usr/local/etc/pulse/default.pa

# Enable network access from Docker containers
load-module module-native-protocol-tcp auth-ip-acl=127.0.0.1;172.16.0.0/12 auth-anonymous=1

# Keep daemon running
load-module module-suspend-on-idle timeout=0
EOF
    
    # Create client.conf for better compatibility
    cat > "$config_dir/client.conf" << 'EOF'
# Client configuration
autospawn = yes
daemon-binary = /usr/local/bin/pulseaudio
EOF
    
    echo "✅ Configuration created at $config_dir"
}

# Function to start PulseAudio daemon
start_pulseaudio() {
    echo "🚀 Starting PulseAudio daemon..."
    
    # Kill existing PulseAudio if running
    if check_pulseaudio_running; then
        echo "   Stopping existing PulseAudio..."
        killall pulseaudio 2>/dev/null || true
        sleep 2
    fi
    
    # Start PulseAudio with network module
    pulseaudio --start --load="module-native-protocol-tcp auth-ip-acl=127.0.0.1;172.16.0.0/12 auth-anonymous=1" --exit-idle-time=-1
    
    # Wait for daemon to start
    sleep 2
    
    if check_pulseaudio_running; then
        echo "✅ PulseAudio daemon started successfully"
    else
        echo "❌ Failed to start PulseAudio daemon"
        echo "   Try running manually:"
        echo "   pulseaudio --start --verbose"
        exit 1
    fi
}

# Function to test audio
test_audio() {
    echo "🔊 Testing audio setup..."
    
    # Check if paplay is available
    if command -v paplay >/dev/null 2>&1; then
        # Try to list sinks
        if pactl list sinks short >/dev/null 2>&1; then
            echo "✅ PulseAudio is working!"
            echo ""
            echo "📋 Available audio outputs:"
            pactl list sinks short
        else
            echo "⚠️  PulseAudio is running but cannot list audio devices"
        fi
    else
        echo "⚠️  PulseAudio tools not found in PATH"
    fi
}

# Function to create start/stop scripts
create_helper_scripts() {
    local script_dir="$HOME/.local/bin"
    mkdir -p "$script_dir"
    
    # Create start script
    cat > "$script_dir/start-docker-audio" << 'EOF'
#!/bin/bash
# Start PulseAudio for Docker containers

echo "🎵 Starting Docker audio service..."

# Start PulseAudio with network access
pulseaudio --start \
    --load="module-native-protocol-tcp auth-ip-acl=127.0.0.1;172.16.0.0/12 auth-anonymous=1" \
    --exit-idle-time=-1 \
    2>/dev/null

if pgrep -x "pulseaudio" > /dev/null; then
    echo "✅ Audio service started"
    echo "   Containers can use: PULSE_SERVER=tcp:host.docker.internal:4713"
else
    echo "❌ Failed to start audio service"
    exit 1
fi
EOF
    chmod +x "$script_dir/start-docker-audio"
    
    # Create stop script
    cat > "$script_dir/stop-docker-audio" << 'EOF'
#!/bin/bash
# Stop PulseAudio daemon

echo "🛑 Stopping Docker audio service..."
killall pulseaudio 2>/dev/null || echo "   Audio service was not running"
echo "✅ Audio service stopped"
EOF
    chmod +x "$script_dir/stop-docker-audio"
    
    echo "📁 Helper scripts created:"
    echo "   • $script_dir/start-docker-audio"
    echo "   • $script_dir/stop-docker-audio"
    
    # Add to PATH if not already there
    if [[ ":$PATH:" != *":$script_dir:"* ]]; then
        echo ""
        echo "💡 Add to your shell profile to use helper commands:"
        echo "   export PATH=\"\$PATH:$script_dir\""
    fi
}

# Main setup flow
main() {
    echo "This script will set up audio support for Docker containers on macOS"
    echo ""
    
    # Check if PulseAudio is installed
    if ! command -v pulseaudio >/dev/null 2>&1; then
        echo "📦 PulseAudio is not installed"
        read -p "Install PulseAudio now? (y/n) " -n 1 -r
        echo
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            install_pulseaudio
        else
            echo "❌ Setup cancelled - PulseAudio is required"
            exit 1
        fi
    else
        echo "✅ PulseAudio is already installed"
    fi
    
    # Configure PulseAudio
    configure_pulseaudio
    
    # Start PulseAudio
    start_pulseaudio
    
    # Test audio
    test_audio
    
    # Create helper scripts
    create_helper_scripts
    
    echo ""
    echo "========================================="
    echo "✅ macOS Docker audio setup complete!"
    echo ""
    echo "📋 Quick reference:"
    echo "   • Start audio: start-docker-audio"
    echo "   • Stop audio:  stop-docker-audio"
    echo "   • In container: PULSE_SERVER=tcp:host.docker.internal:4713"
    echo ""
    echo "💡 Your containers will now have audio support!"
    echo "========================================="
}

# Run main function
main