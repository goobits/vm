# VM Infrastructure

Configuration-driven development environment using Vagrant + Ansible with beautiful terminal themes.

## 🚀 Quick Start

```bash
# 1. Copy this folder to your project
cp -r packages/vm your-project/packages/

# 2. Create vm.json in project root with your configuration
# See vm.jsonc for example with comments
# At minimum, define your ports

# 3. Add script to your package.json
{
  "scripts": {
    "vm": "./packages/vm/vm.sh"
  }
}

# 4. Start VM
yarn vm up      # Creates VM, installs everything

# 5. Enter VM
yarn vm ssh     # You're now in Ubuntu with Node.js + beautiful terminal!
```
## 📦 Included Software

- **Operating System**: Ubuntu 22.04 (configurable)
- **Shell**: Zsh with syntax highlighting + custom prompts
- **Node.js**: Configurable version via NVM (default: v22)
- **Yarn**: Configurable version via Corepack (default: 4.9.1)
- **PostgreSQL**: Optional service (default: disabled)
- **Redis**: Optional service (default: disabled) 
- **MongoDB**: Optional service (default: disabled)
- **Docker**: Optional service (default: enabled)
- **Headless Browser**: Optional for testing (default: disabled)

## 🎨 Terminal Themes

Choose from 8 beautiful, popular themes:

- **`dracula`** ⭐ **Default** - Purple accents, vibrant colors
- **`gruvbox_dark`** - Warm, earthy retro tones
- **`solarized_dark`** - Precision colors, scientifically designed
- **`nord`** - Arctic, north-bluish palette
- **`monokai`** - Classic dark with vibrant highlights
- **`one_dark`** - Atom's iconic theme
- **`catppuccin_mocha`** - Soothing pastels
- **`tokyo_night`** - Clean, inspired by Tokyo's night

All themes include syntax highlighting and custom prompts!

## 🎯 Default Configuration

The package includes `packages/vm/vm.json` with sensible defaults. You can use it as-is for most projects, or create your own `vm.json` in the project root to override specific values.

### Minimal Override Example

```json
{
  "project": {
    "name": "my-awesome-app"
  },
  "ports": {
    "frontend": 3020,
    "backend": 3022,
    "postgresql": 3025
  },
  "services": {
    "postgresql": { "enabled": true }
  },
  "terminal": {
    "emoji": "⚡",
    "username": "awesome",
    "theme": "nord"
  }
}
```

This minimal config will:
- Use all defaults from `packages/vm/vm.json`
- **Define ports (required)** - no default ports are included
- Override project name and terminal customization
- Enable PostgreSQL (disabled by default)
- Get a custom terminal: `⚡ awesome my-awesome-app >`

## 🔧 Configuration

Complete `vm.json` reference:

```json
{
  "project": {
    "name": "my-app",
    "hostname": "dev.my-app.local",
    "workspace_path": "/workspace",
    "env_template_path": null,  // e.g. "backend/.env.template"
    "backup_pattern": "*backup*.sql.gz"
  },
  "vm": {
    "box": "bento/ubuntu-22.04",
    "memory": 4096,
    "cpus": 2,
    "user": "vagrant"
  },
  "versions": {
    "node": "22",
    "nvm": "v0.39.7",
    "yarn": "4.9.1"
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
      "password": "postgres",
      "port": 5432
    },
    "redis": { 
      "enabled": true,
      "port": 6379
    },
    "mongodb": { 
      "enabled": false,
      "port": 27017
    },
    "docker": { "enabled": true },
    "headless_browser": { 
      "enabled": true,
      "display": ":99",
      "executable_path": "/usr/bin/chromium-browser"
    }
  },
  "npm_packages": [
    "prettier",
    "eslint",
    "typescript"
  ],
  "environment": {
    "NODE_ENV": "development",
    "API_URL": "http://localhost:3001"
  },
  "terminal": {
    "emoji": "🚀",
    "username": "dev",
    "theme": "dracula",
    "show_git_branch": true,
    "show_timestamp": false
  }
}
```

### Terminal Configuration

The `terminal` section lets you customize your shell experience:

```json
{
  "terminal": {
    "emoji": "🌀",
    "username": "myproject",
    "theme": "dracula", 
    "show_git_branch": true,
    "show_timestamp": false
  }
}
```

**Options:**
- `emoji`: Custom emoji for prompt
- `username`: Display name in terminal  
- `theme`: Theme name (see themes above)
- `show_git_branch`: Show current git branch
- `show_timestamp`: Show current time

**Result**: `🌀 myproject goobits (main) >`

Available themes: `dracula`, `gruvbox_dark`, `solarized_dark`, `nord`, `monokai`, `one_dark`, `catppuccin_mocha`, `tokyo_night`

## 🛠️ VM Management

```bash
yarn vm status   # Check if running
yarn vm halt     # Stop VM (preserves data)
yarn vm destroy  # Delete VM completely
yarn vm reload   # Restart VM with new config
yarn vm provision # Re-run Ansible provisioning
```

## 📁 File Sync

Edit anywhere - changes sync instantly:
```
Mac: ~/your-project/src/app.js
 ↕️
VM:  /workspace/src/app.js
```

## 🔌 Port Configuration

### Port Range Convention
To avoid conflicts when running multiple projects, we recommend assigning each project a dedicated port range of 10 ports:

- **Project 1**: 3000-3009
- **Project 2**: 3010-3019  
- **Project 3**: 3020-3029
- **Project 4**: 3030-3039
- etc.

Example port allocation for a project using 3020-3029:
```json
{
  "ports": {
    "frontend": 3020,        // Main web app
    "frontend_preview": 3021, // Production preview
    "backend": 3022,         // API server
    "admin": 3023,           // Admin dashboard
    "admin_dev": 3024,       // Admin dev server
    "postgresql": 3025,      // Database
    "redis": 3026,           // Cache/queue
    "devtools": 3027,        // Chrome DevTools
    "docs": 3028,            // Documentation site
    "storybook": 3029        // Component library
  }
}
```

### Port Forwarding
By default, forwarded ports are only accessible from localhost (127.0.0.1) for security. 

To make ports accessible from other machines on your network, add this to your `vm.json`:
```json
{
  "vm": {
    "port_binding": "0.0.0.0"  // WARNING: Exposes services to network
  }
}
```

This allows you to:
- Access from your machine's IP address (e.g., `192.168.1.100:3020`)
- Share development URLs with others on your network
- Test on mobile devices

### Port Conflicts
If you see "port collision", check the output:
```
==> default: Fixed port collision for 3000 => 3000. Now on port 2200.
```
Your app is now on `localhost:2200` instead.


## 🔒 Claude Settings

The `claude-settings/settings.json` file contains security settings for Claude Code. Update the paths to match your `workspace_path`:

```json
{
  "allow": [
    "Read(/your-workspace/**)",
    "Write(/your-workspace/**)",
    // etc.
  ]
}
```

## 🔒 Security Best Practices

1. **Port Binding**: Default is localhost-only (127.0.0.1). Only use "0.0.0.0" if you need network access.
2. **Claude Settings**: The included settings.json restricts dangerous commands. Customize as needed.
3. **Database Passwords**: Change default PostgreSQL passwords for any non-development use.
4. **SSH Keys**: VM uses Vagrant's insecure key by default. This is fine for local development.

## ⚠️ Troubleshooting

- **Port conflicts**: Check the Vagrant output for remapped ports
- **VM won't start**: Run `yarn vm destroy` and try again
- **Slow performance**: Increase memory/CPUs in vm.json
- **Can't access ports**: Ensure services are running and ports are in vm.json

## ✅ Requirements

- Vagrant 2.0+
- VirtualBox/Parallels/VMware
- 4GB free RAM