# 🚀 VM Infrastructure

Beautiful development VMs with one command. Vagrant + Ansible + gorgeous terminals.

> **🔐 Built for Claude Code**: This VM infrastructure was specifically created to provide a safe sandbox where you can run `claude --dangerously-skip-permissions` without fear. Unlike Docker (which shares your host kernel), Vagrant VMs provide true isolation with their own kernel - meaning even if something goes wrong, it stays contained in the VM. We use Docker _inside_ the VM for project containers, giving you the best of both worlds: security + convenience.

## 📚 Table of Contents

- [Quick Start](#-quick-start)
- [What's Included](#-whats-included)
- [Terminal Themes](#-terminal-themes)
- [Configuration](#-configuration)
    - [Minimal Setup](#-minimal-setup)
    - [Full Reference](#-full-reference)
    - [Terminal Options](#-terminal-options)
- [Commands](#-commands)
- [Port Strategy](#-port-strategy)
- [Tips & Tricks](#-tips--tricks)
- [Troubleshooting](#-troubleshooting)
- [Installation](#-installation)

## 🏃 Quick Start

```bash
# 1. Copy to your project
cp -r packages/vm your-project/packages/

# 2. Add to package.json
{
  "scripts": {
    "vm": "./packages/vm/vm.sh"
  }
}

# 3. Create minimal vm.json (or use defaults!)
{
  "ports": {
    "frontend": 3000,
    "backend": 3001
  }
}

# 4. Launch!
pnpm vm up      # Creates & provisions VM
pnpm vm ssh     # Enter your shiny new Ubuntu box
```

## 📦 What's Included

- **Ubuntu 22.04** with Zsh + syntax highlighting
- **Node.js v22** via NVM (configurable)
- **pnpm** via Corepack
- **Beautiful terminals** with 8 themes
- **Optional services**: PostgreSQL, Redis, MongoDB, Docker, Headless Browser
- **Auto-sync**: Edit locally, run in VM
- **Claude-ready**: Safe sandbox for AI experiments

## 🎨 Terminal Themes

All themes include syntax highlighting and git-aware prompts!

- `dracula` ⭐ - Purple magic (default)
- `gruvbox_dark` - Retro warmth
- `solarized_dark` - Science-backed colors
- `nord` - Arctic vibes
- `monokai` - Classic vibrance
- `one_dark` - Atom's gift
- `catppuccin_mocha` - Smooth pastels
- `tokyo_night` - Neon dreams

## ⚙️ Configuration

### 🎯 Minimal Setup

Most projects just need ports. Everything else has smart defaults:

```json
{
	"ports": {
		"frontend": 3020,
		"backend": 3022
	}
}
```

Want PostgreSQL? Just add:

```json
{
	"ports": {
		"frontend": 3020,
		"backend": 3022,
		"postgresql": 3025
	},
	"services": {
		"postgresql": { "enabled": true }
	}
}
```

### 📋 Full Reference

```json
{
	"project": {
		"name": "my-app", // VM name & prompt
		"hostname": "dev.my-app.local", // VM hostname
		"workspace_path": "/workspace", // Sync path in VM
		"env_template_path": null, // e.g. "backend/.env.template"
		"backup_pattern": "*backup*.sql.gz" // For auto-restore
	},
	"vm": {
		"box": "bento/ubuntu-22.04", // Vagrant box
		"memory": 4096, // RAM in MB
		"cpus": 2, // CPU cores
		"user": "vagrant", // VM user
		"port_binding": "127.0.0.1" // or "0.0.0.0" for network
	},
	"versions": {
		"node": "22", // Node version
		"nvm": "v0.39.7", // NVM version
		"pnpm": "latest" // pnpm version
	},
	"ports": {
		"frontend": 3000,
		"backend": 3001,
		"postgresql": 5432,
		"redis": 6379
	},
	"services": {
		"postgresql": {
			"enabled": true,
			"database": "myapp_dev",
			"user": "postgres",
			"password": "postgres"
		},
		"redis": { "enabled": true },
		"mongodb": { "enabled": false },
		"docker": { "enabled": true },
		"headless_browser": { "enabled": false }
	},
	"npm_packages": [
		// Global installs
		"prettier",
		"eslint"
	],
	"aliases": {
		// Custom aliases
		"dev": "pnpm dev",
		"test": "pnpm test"
	},
	"environment": {
		// ENV vars
		"NODE_ENV": "development"
	}
}
```

### 🎭 Terminal Options

Make your prompt uniquely yours:

```json
{
	"terminal": {
		"emoji": "⚡", // Prompt emoji
		"username": "hacker", // Prompt name
		"theme": "tokyo_night", // Color theme
		"show_git_branch": true, // Show branch
		"show_timestamp": false // Show time
	}
}
```

Result: `⚡ hacker my-app (main) >`

## 🎮 Commands

```bash
pnpm vm up         # Start VM
pnpm vm ssh        # Connect to VM
pnpm vm halt       # Stop VM (keeps data)
pnpm vm destroy    # Delete VM
pnpm vm reload     # Restart with new config
pnpm vm status     # Check if running
pnpm vm validate   # Check config
pnpm vm kill       # Force kill stuck VMs
pnpm vm provision  # Re-run Ansible provisioning
```

## 🔌 Port Strategy

Avoid conflicts by giving each project 10 ports:

- **Project 1**: 3000-3009
- **Project 2**: 3010-3019
- **Project 3**: 3020-3029
- **Project 4**: 3030-3039

Example allocation:

```json
{
	"ports": {
		"frontend": 3020, // Main app
		"backend": 3022, // API
		"postgresql": 3025, // Database
		"redis": 3026, // Cache
		"docs": 3028 // Documentation
	}
}
```

**Network access?** Add `"port_binding": "0.0.0.0"` to share with your network.

## 💡 Tips & Tricks

### 🔄 File Sync

```
Mac: ~/your-project/src/app.js
 ↕️ (instant sync)
VM:  /workspace/src/app.js
```

### 🧪 Why Vagrant for Claude Code?

**Security layers**:

1. **VM isolation**: Separate kernel = true sandbox (unlike Docker's shared kernel)
2. **Claude can experiment freely**: Install packages, modify configs, test ideas
3. **Your host stays safe**: Even with `--dangerously-skip-permissions`
4. **Docker inside VM**: Best practice for container security (disabled by default, enable in config)

The only restrictions prevent VM shutdown/reboot (would disconnect Claude).

### 🐘 Database Backups

Drop `.sql.gz` files matching your `backup_pattern` in the project - they'll auto-restore on provision!

### 🚪 Port Conflicts

See "port collision" in output? Vagrant auto-remapped it:

```
Fixed port collision for 3000 => 3000. Now on port 2200.
```

## 🚨 Troubleshooting

**Q: Port conflicts?**  
A: Check Vagrant output for remapped ports

**Q: VM won't start?**  
A: `pnpm vm destroy` then `pnpm vm up`

**Q: Slow performance?**  
A: Increase memory/CPUs in vm.json

**Q: Can't connect to service?**  
A: Check it's enabled and port is in vm.json

**Q: VirtualBox stuck?**  
A: `pnpm vm kill` to force cleanup

## 💻 Installation

### macOS

```bash
brew tap hashicorp/tap
brew install hashicorp/tap/hashicorp-vagrant
brew install --cask virtualbox
```

### Ubuntu/Debian

```bash
# Add HashiCorp repo
wget -O- https://apt.releases.hashicorp.com/gpg | \
  sudo gpg --dearmor -o /usr/share/keyrings/hashicorp-archive-keyring.gpg
echo "deb [signed-by=/usr/share/keyrings/hashicorp-archive-keyring.gpg] \
  https://apt.releases.hashicorp.com $(lsb_release -cs) main" | \
  sudo tee /etc/apt/sources.list.d/hashicorp.list

# Install
sudo apt update && sudo apt install vagrant virtualbox
```

### Windows

Download from [vagrant.com](https://www.vagrantup.com/downloads) and [virtualbox.org](https://www.virtualbox.org/wiki/Downloads)

---

**Pro tip**: The package includes `packages/vm/vm.json` with sensible defaults. Your project's `vm.json` only needs what's different! 🎪
