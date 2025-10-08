# 📚 Complete CLI Reference

The `pkg-server` command-line interface provides everything you need to run, configure, and manage your local package server.

## 🎯 **Main Command**

```bash
pkg-server [COMMAND] [OPTIONS]
```

## 📋 **Commands Overview**

| Command | Description | Quick Example |
|---------|-------------|---------------|
| `start` | Start the server | `pkg-server start` |
| `status` | Show server status and stats | `pkg-server status` |
| `add` | Publish package from current directory | `pkg-server add` |
| `remove` | Delete package from server | `pkg-server remove` |
| `list` | List all packages on server | `pkg-server list` |

---

## 🚀 **start** - Start Server

Start the package server and optionally configure local package managers.

### **Usage**
```bash
pkg-server start [OPTIONS]
```

### **Options**
| Option | Short | Default | Description |
|--------|-------|---------|-------------|
| `--host` | | `0.0.0.0` | Host/IP to bind server to |
| `--port` | `-p` | `3080` | Port to run server on |
| `--data` | | `./data` | Directory for package storage |

### **Examples**
```bash
# Start with defaults
pkg-server start

# Start on custom port
pkg-server start --port 9000

# Start with a specific data directory
pkg-server start --data /var/lib/packages
```

---

## 📦 **add** - Publish Package

Publish a package from the current directory to the server.

### **Usage**
```bash
pkg-server add [OPTIONS]
```

### **Options**
| Option | Default | Description |
|--------|---------|-------------|
| `--server` | `http://localhost:3080` | Server URL |
| `--type` | Auto-detect | Package type(s) to publish (python,npm,cargo) |

### **Examples**
```bash
# In a Python project directory (auto-detects package type)
pkg-server add

# Publish specific package types only
pkg-server add --type python,npm

# Specify remote server
pkg-server add --server http://192.168.1.100:3080
```

### **Supported Package Types:**
- **Python**: Detects `setup.py` or `pyproject.toml`
- **Node.js**: Detects `package.json`
- **Rust**: Detects `Cargo.toml`

---

## 🗑️ **remove** - Delete Package

Remove a package from the server.

### **Usage**
```bash
pkg-server remove [OPTIONS]
```

### **Options**
| Option | Default | Description |
|--------|---------|-------------|
| `--server` | `http://localhost:3080` | Server URL |

### **Interactive Prompts:**
- Asks for package type (PyPI/NPM/Cargo)
- Asks for package name to delete

---

## 📋 **list** - List Packages

List all packages stored on the server.

### **Usage**
```bash
pkg-server list [OPTIONS]
```

### **Options**
| Option | Default | Description |
|--------|---------|-------------|
| `--server` | `http://localhost:3080` | Server URL |

### **Output Format:**
```
PyPI packages:
  - requests
  - pandas
  - numpy

NPM packages:
  - express
  - lodash
  - axios

Cargo packages:
  - serde
  - tokio
  - clap
```

---

## 📊 **status** - Server Status

Show server status, version, and package counts.

### **Usage**
```bash
pkg-server status [OPTIONS]
```

### **Options**
| Option | Default | Description |
|--------|---------|-------------|
| `--server` | `http://localhost:3080` | Server URL |

### **Output Example:**
```json
{
  "status": "running",
  "server_addr": "http://0.0.0.0:3080",
  "data_dir": "./data",
  "version": "0.1.0",
  "packages": {
    "pypi": 42,
    "npm": 156,
    "cargo": 23
  }
}
```

---

## 🌐 **HTTP Endpoints**

The server also provides these HTTP endpoints:

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/setup.sh` | GET | Auto-generated setup script |
| `/api/status` | GET | Server status JSON |
| `/` | GET | Web UI homepage |

### **Quick Setup via HTTP:**
```bash
# Configure any machine with one command
curl http://SERVER_IP:3080/setup.sh | bash
```

---

## 🎯 **Common Workflows**

### **Start Server for Team**
```bash
# On server machine
pkg-server start --docker --port 80

# On team machines
curl http://team-server/setup.sh | bash
```

### **Development Mode**
```bash
# Local development
pkg-server start --no-config

# Test in Docker
pkg-server start --docker --port 8081
```

### **CI/CD Integration**
```bash
# In CI pipeline
pkg-server start --docker --data /cache/packages
curl http://localhost:3080/setup.sh | bash
# Now all package installs use cache
```

### **Offline Development**
```bash
# Pre-populate cache while online
pkg-server start
pip install -r all-requirements.txt
npm install

# Later, work offline with cached packages
pkg-server start --no-config
```

---

## 💾 **Data Storage**

### **Package Locations:**
- **PyPI**: `{data_dir}/pypi/packages/`
- **NPM**: `{data_dir}/npm/tarballs/` and `{data_dir}/npm/metadata/`
- **Cargo**: `{data_dir}/cargo/crates/` and `{data_dir}/cargo/index/`

### **Backup:**
Simply backup the data directory to preserve all cached packages.

---

## 🔐 **Environment Variables**

| Variable | Description |
|----------|-------------|
| `RUST_LOG` | Set log level (e.g., `info`, `debug`) |
| `PKG_SERVER_DATA` | Override default data directory |

---

## ⚡ **Tips & Tricks**

1. **Auto-start on boot** (systemd):
```bash
# /etc/systemd/system/pkg-server.service
[Unit]
Description=Package Server - Universal Package Index
After=network.target

[Service]
ExecStart=/usr/local/bin/pkg-server start --docker
Restart=always
RestartSec=10
User=pkg-server
Group=pkg-server

[Install]
WantedBy=multi-user.target
```

2. **Multiple servers** (different ports):
```bash
pkg-server start --port 3080  # PyPI focused
pkg-server start --port 8081  # NPM focused
```

3. **Quick health check**:
```bash
curl -f http://localhost:3080/api/status || echo "Server is down"
```

4. **Bulk package pre-loading**:
```bash
# Pre-cache common packages
for pkg in requests pandas numpy; do
  pip install --index-url http://localhost:3080/pypi/simple/ $pkg
done
```

---

This is your complete CLI - simple, powerful, and Docker-optional! 🚀