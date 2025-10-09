# 🔧 Troubleshooting Guide

Common issues and solutions for the VM development environment.

## 🚨 Quick Fixes

### VM Won't Start
```bash
# Try the universal fix first
vm destroy && vm create

# Check if services are running
docker ps -a  # For Docker provider
vagrant status  # For Vagrant provider

# Check available resources
docker system df  # Disk space
docker system prune  # Clean up
```

### Can't Connect to VM
```bash
# Check VM status
vm status

# Try restarting
vm stop && vm start

# Check SSH configuration
vm ssh -v  # Verbose SSH output
```

### Port Conflicts
```bash
# Find what's using the port
lsof -i :3000  # Replace 3000 with your port
netstat -tulpn | grep :3000

# Update your configuration
# Edit vm.yaml and change conflicting ports
vm destroy && vm create
```

## 🐳 Docker Issues

### Docker Permission Denied
```bash
# Add user to docker group (Linux)
sudo usermod -aG docker $USER
newgrp docker

# Restart Docker service
sudo systemctl restart docker

# Verify access
docker ps
```

### Docker Desktop Not Running (macOS/Windows)
1. Start Docker Desktop application
2. Wait for Docker to fully initialize
3. Check system tray for Docker icon
4. Try `docker ps` to verify

### Container Resource Issues
```bash
# Check Docker resources
docker system df
docker container ls -a

# Clean up unused containers
docker container prune
docker image prune
docker volume prune

# Increase Docker resources in Docker Desktop settings
# Memory: 4GB+ recommended
# CPUs: 2+ recommended
```

### Container Won't Start
```bash
# Check container logs. Get container name from `vm status`.
docker logs <container_name>

# Check for port conflicts
docker port <container_name>

# Restart with fresh container
vm destroy && vm create
```

## 📦 Vagrant Issues

### VirtualBox Conflicts
```bash
# Disable Hyper-V (Windows)
bcdedit /set hypervisorlaunchtype off
# Reboot required

# Check VirtualBox installation
VBoxManage --version

# Update VirtualBox if version conflicts exist
```

### VM Creation Timeouts
```bash
# Increase timeout in vm.yaml
vm:
  timeout: 600  # 10 minutes

# Check available disk space
df -h

# Check available memory
free -h
```

### SSH Connection Issues
```bash
# Check SSH key permissions
ls -la ~/.ssh/

# Regenerate SSH keys if needed
ssh-keygen -t rsa -b 4096

# Check Vagrant SSH config
vagrant ssh-config
```

### Box Download Failures
```bash
# Clear Vagrant cache
rm -rf ~/.vagrant.d/boxes/*

# Try different box mirror
vagrant box add bento/ubuntu-24.04 --provider virtualbox --force

# Check network connectivity
ping -c 4 vagrantcloud.com
```

## 🍎 Tart Issues (Apple Silicon)

### Tart Not Found
```bash
# Install Tart
brew install cirruslabs/cli/tart

# Verify installation
tart --version
which tart
```

### VM Boot Failures
```bash
# Check available disk space (Tart needs significant space)
df -h

# List available images
tart list

# Pull fresh image
tart pull ghcr.io/cirruslabs/macos-sonoma-base:latest
```

### SSH Connection Problems
```bash
# Check if SSH is enabled in VM
tart run <vm-name> -- systemctl status ssh

# Verify SSH key setup
cat ~/.ssh/id_rsa.pub
```

## ⚙️ Configuration Issues

### Invalid Configuration
```bash
# Validate your configuration
vm validate

# Check for syntax errors
yaml-lint vm.yaml  # If you have yamllint installed

# Use minimal config to test
echo "os: ubuntu" > vm.yaml
vm create
```

### Preset Detection Problems
```bash
# Apply a preset manually if detection fails
vm config preset nodejs

# Apply specific preset manually
vm config preset django

# Check detection logic
LOG_LEVEL=DEBUG vm create
```

### Service Configuration Issues
```bash
# Test individual services
docker run -d --name test-postgres postgres:13
docker logs test-postgres

# Check service dependencies
vm exec "systemctl status postgresql"
vm exec "systemctl status redis"

# Restart services
vm provision  # Re-run provisioning scripts
```

## 🗄️ Database Issues

### PostgreSQL Won't Start
```bash
# Check PostgreSQL status
vm exec "systemctl status postgresql"

# Check PostgreSQL logs
vm exec "tail -f /var/log/postgresql/postgresql-*.log"

# Reset PostgreSQL data
vm exec "sudo rm -rf /var/lib/postgresql/*/main && sudo -u postgres initdb -D /var/lib/postgresql/*/main"
```

### Redis Connection Refused
```bash
# Check Redis status
vm exec "systemctl status redis"

# Test Redis connection
vm exec "redis-cli ping"

# Check Redis configuration
vm exec "cat /etc/redis/redis.conf | grep bind"
```

### Database Data Lost
```bash
# Enable database persistence in vm.yaml
# Note: This is a deprecated top-level field but still supported.
# The modern approach is to configure persistence per-service.
persist_databases: true

# Recreate VM with persistence
vm destroy && vm create

# Check if data directory exists
ls -la .vm/data/
```

## 🌐 Network Issues

### Can't Access Services from Host
```bash
# Check port forwarding
vm status  # Shows port mappings

# Verify service is listening
vm exec "netstat -tulpn | grep :3000"

# Check firewall settings
vm exec "sudo ufw status"

# Test direct connection
curl http://localhost:3000
```

### Services Not Accessible from Network
```yaml
# Update vm.yaml for network access
vm:
  port_binding: "0.0.0.0"  # Instead of 127.0.0.1

# Recreate VM
vm destroy && vm create
```

### DNS Resolution Issues
```bash
# Check /etc/hosts
vm exec "cat /etc/hosts"

# Test DNS resolution
vm exec "nslookup google.com"
vm exec "dig google.com"

# Update DNS servers
vm exec "echo 'nameserver 8.8.8.8' >> /etc/resolv.conf"
```

## 📁 File Sync Issues

### Files Not Syncing
```bash
# Check mount points
vm exec "mount | grep workspace"
df -h  # Check for mount issues

# Restart file sync (Docker). Get container name from `vm status`.
docker restart <container_name>

# Restart file sync (Vagrant)
vagrant reload
```

### Permission Errors
```bash
# Check file permissions
ls -la ./

# Fix ownership in VM
vm exec "sudo chown -R \$USER:\$USER /workspace"

# Check mount options
vm exec "mount | grep workspace"
```

### Slow File Operations
```bash
# For Docker on macOS, use cached mounts
# Add to vm.yaml:
mounts:
  - "./:/workspace:cached"

# For Vagrant, ensure guest additions are installed
vagrant plugin install vagrant-vbguest
vagrant reload
```

## 🔍 Debugging Mode

### Enable Debug Output
```bash
# Verbose VM tool output
LOG_LEVEL=DEBUG vm create

# Shell script debugging
VM_DEBUG=true vm create

# Docker/Vagrant verbose output
VM_VERBOSE=true vm create
```

### VM Tool Path Configuration
```bash
# Set custom VM tool installation directory
export VM_TOOL_DIR=/path/to/vm/installation
vm create

# Check current VM tool directory detection
VM_TOOL_DIR=/custom/path vm create  # Will use custom path
vm create  # Will auto-detect from binary location

# Troubleshoot vm-tool mount issues in Docker
export VM_TOOL_DIR=/Users/username/projects/vm
vm destroy && vm create
```

**VM_TOOL_DIR Environment Variable:**
- **Purpose**: Specifies the VM tool installation directory for container mounting
- **Auto-detection**: Usually detected automatically from binary location
- **When to set**: When binary is installed in non-standard location or via symlink
- **Docker usage**: Mounts this directory to `/vm-tool` in containers for Ansible access

### Inspect VM State
```bash
# Get detailed VM information
vm status --verbose

# Access VM directly. Get container name from `vm status`.
docker exec -it <container_name> /bin/bash  # Docker
vagrant ssh  # Vagrant

# Check running processes
vm exec "ps aux"
vm exec "systemctl status"
```

### Log Analysis
```bash
# View VM tool logs
vm logs

# View system logs in VM
vm exec "journalctl -f"
vm exec "tail -f /var/log/syslog"

# View service-specific logs
vm exec "docker logs redis"  # If using Docker services
```

## 🆘 Getting Help

### System Information
```bash
# Gather system info for support
echo "=== System Info ==="
uname -a
docker --version
vagrant --version 2>/dev/null || echo "Vagrant not installed"
tart --version 2>/dev/null || echo "Tart not installed"

echo "=== VM Status ==="
vm status

echo "=== Configuration ==="
cat vm.yaml
```

### Reset Everything
```bash
# Nuclear option - reset everything
vm destroy
docker system prune -a  # Docker cleanup
rm -rf .vm/  # Remove local VM data
vm create  # Start fresh
```

### Report Issues
When reporting issues, include:
1. Operating system and version
2. Provider being used (Docker/Vagrant/Tart)
3. VM configuration (vm.yaml)
4. Error messages and logs
5. Steps to reproduce

## 💡 Performance Tips

### Speed Up VM Creation
```bash
# Pre-pull base images
docker pull ubuntu:22.04
vagrant box add bento/ubuntu-24.04

# Use minimal configurations for testing
echo "os: alpine" > test.yaml
vm --config test.yaml create
```

### Optimize Resource Usage
```yaml
# Minimal resources for simple projects
vm:
  memory: 2048
  cpus: 1

# More resources for complex projects
vm:
  memory: 8192
  cpus: 4
```

### Reduce Startup Time
```bash
# Disable unnecessary services
services:
  postgresql:
    enabled: false
  redis:
    enabled: false

# Use lighter OS options
os: alpine  # Smallest
os: debian  # Lightweight
os: ubuntu  # Full-featured
```