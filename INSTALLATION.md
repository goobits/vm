# 💻 Installation Guide

Beautiful development environments with one command. Choose between Docker (lightweight containers, default) or Vagrant (full VM isolation) based on your needs.

## 🏃 Quick Start

### Option 1: Clone and Install (Recommended)

```bash
# 1. Clone repository and install globally
git clone <repository-url>
cd vm
./install.sh

# 2. Start immediately with defaults OR create custom vm.yaml
vm create  # Works without any config! Uses smart defaults
vm ssh     # Enter your shiny new Ubuntu box

# OR customize with vm.yaml
```

Create a vm.yaml file (or use `vm init`):
```yaml
project:
  name: my-project
  hostname: dev.my-project.local
ports:
  frontend: 3000
  backend: 3001
# Default provider is Docker - add "provider": "vagrant" for full VM isolation
```

### Option 2: Per-Project Installation

```bash
# 1. Clone to your project directory
git clone <repository-url> vm
cd vm

# 2. Use directly without global installation
./vm.sh create
```

### Option 3: Package.json Integration

```bash
# 1. Clone to your project
git clone <repository-url> vm

# 2. Add to package.json scripts
{
  "scripts": {
    "vm": "./vm/vm.sh"
  }
}

# 3. Launch via package manager
npm run vm create
# or
pnpm vm create
```

## 📋 Prerequisites

### For Docker Provider (Default)
- **Docker Desktop** (macOS/Windows) or **Docker Engine** (Linux)
- **docker-compose**
- **yq v4+** (mikefarah/yq - YAML processor)
- **Python3** (macOS only - for cross-platform path operations)

### For Vagrant Provider
- **VirtualBox** or **Parallels**
- **Vagrant**

## 🍎 macOS Installation

### Docker Provider
```bash
# Install Docker Desktop
brew install --cask docker

# Install YAML processor (mikefarah/yq v4+)
brew install yq
```

### Vagrant Provider  
```bash
# Install Vagrant and VirtualBox
brew tap hashicorp/tap
brew install hashicorp/tap/hashicorp-vagrant
brew install --cask virtualbox
```

## 🐧 Ubuntu/Debian Installation

### Docker Provider
```bash
# Install Docker
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker $USER

# Install yq (mikefarah/yq v4+)
# Remove old Python yq if installed
sudo apt remove yq 2>/dev/null || true
# Detect architecture and install appropriate yq binary
ARCH=$(uname -m)
case "$ARCH" in
    x86_64) YQ_ARCH="amd64" ;;
    aarch64|arm64) YQ_ARCH="arm64" ;;
    *) 
        echo "❌ Error: Unsupported Linux architecture: $ARCH" >&2
        echo "Supported: x86_64, aarch64/arm64" >&2
        exit 1
        ;;
esac
sudo wget -qO /usr/local/bin/yq "https://github.com/mikefarah/yq/releases/latest/download/yq_linux_${YQ_ARCH}"
sudo chmod +x /usr/local/bin/yq

# Log out and back in for docker group changes to take effect
```

### Vagrant Provider
```bash
# Add HashiCorp GPG key
wget -O- https://apt.releases.hashicorp.com/gpg | \
  sudo gpg --dearmor -o /usr/share/keyrings/hashicorp-archive-keyring.gpg

# Add HashiCorp repository
echo "deb [signed-by=/usr/share/keyrings/hashicorp-archive-keyring.gpg] \
  https://apt.releases.hashicorp.com $(lsb_release -cs) main" | \
  sudo tee /etc/apt/sources.list.d/hashicorp.list

# Install packages
sudo apt update && sudo apt install vagrant virtualbox
```

## 🪟 Windows Installation

### Docker Provider
1. Download and install [Docker Desktop](https://www.docker.com/products/docker-desktop)
2. Install yq via package manager or download binary

### Vagrant Provider
1. Download **Vagrant** from [vagrant.com](https://www.vagrantup.com/downloads)
2. Download **VirtualBox** from [virtualbox.org](https://www.virtualbox.org/wiki/Downloads)

## ✅ Verification

After installation, verify everything works:

```bash
# Check Docker (if using Docker provider)
docker --version
docker-compose --version
yq --version

# Check Vagrant (if using Vagrant provider)  
vagrant --version
VBoxManage --version

# Test VM tool
vm --help
vm create  # Should work with defaults
```

## 🚨 Troubleshooting Installation

### Docker Issues
- **macOS/Windows**: Ensure Docker Desktop is running
- **Linux**: Check if docker service is started: `sudo systemctl start docker`
- **Permissions**: Make sure your user is in the docker group: `groups | grep docker`

### yq Issues
- **Wrong yq version**: Make sure you have mikefarah/yq v4+, not kislyuk/yq (Python version)
- **Check version**: `yq --version` should show v4+ without "yq (https://github.com/kislyuk/yq)"
- **Manual install**: Download from [mikefarah/yq releases](https://github.com/mikefarah/yq/releases)

### Vagrant Issues
- **VirtualBox conflicts**: Disable Hyper-V on Windows, or use Parallels on macOS
- **Permissions**: On Linux, add user to vboxusers group: `sudo usermod -aG vboxusers $USER`

### General Issues
- **Path problems**: Make sure the vm command is in your PATH after global installation
- **Permission denied**: Check that install.sh is executable: `chmod +x install.sh`

## 🔄 Updating

### npm Installation
```bash
npm update -g @goobits/vm
```

### Manual Installation
```bash
cd vm
git pull
./install.sh
```

## 🗑️ Uninstallation

### npm Installation
```bash
npm uninstall -g @goobits/vm
```

### Manual Installation
```bash
# Remove from PATH (edit your shell profile)
# Remove the installed directory
rm -rf /usr/local/bin/vm  # or wherever it was installed
```