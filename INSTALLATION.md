# 💻 Installation Guide

Beautiful development environments with one command. Choose between Docker (lightweight containers, default) or Vagrant (full VM isolation) based on your needs.

## 🏃 Quick Start

### Option 1: npm Global Installation (Recommended)

```bash
# 1. Install globally via npm
npm install -g @goobits/vm

# 2. Start immediately with defaults OR create custom vm.yaml
vm create  # Works without any config! Uses smart defaults
vm ssh     # Enter your shiny new Ubuntu box

# OR customize with vm.yaml
```

Create a vm.yaml file (or use `vm init`):
```yaml
project:
  name: my-project
ports:
  frontend: 3000
  backend: 3001
# Default provider is Docker - add "provider": "vagrant" for full VM isolation
```

### Option 2: Manual Global Installation

```bash
# 1. Clone and install
git clone <repo-url>
cd vm
./install.sh

# 2. Use globally
vm create
```

### Option 3: Per-Project Installation

```bash
# 1. Copy to your project
cp -r vm your-project/

# 2. Add to package.json
{
  "scripts": {
    "vm": "./vm/vm.sh"
  }
}

# 3. Launch!
pnpm vm create
```

## 📋 Prerequisites

### For Docker Provider (Default)
- **Docker Desktop** (macOS/Windows) or **Docker Engine** (Linux)
- **docker-compose**
- **yq** (YAML processor)

### For Vagrant Provider
- **VirtualBox** or **Parallels**
- **Vagrant**

## 🍎 macOS Installation

### Docker Provider
```bash
# Install Docker Desktop
brew install --cask docker

# Install YAML processor
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

# Install yq
sudo apt-get update && sudo apt-get install yq

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
- **Ubuntu older than 20.04**: Install from [GitHub releases](https://github.com/mikefarah/yq/releases)
- **Snap installation**: `sudo snap install yq`

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