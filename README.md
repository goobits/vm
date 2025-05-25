# VM Infrastructure

Configuration-driven development environment using Vagrant + Ansible with beautiful terminal themes.

## 🚀 Quick Start

```bash
# 1. Copy this folder to your project
cp -r packages/vm your-project/packages/

# 2. (Optional) Create vm.json in project root to override defaults
# The package includes sensible defaults with Dracula theme
# You only need to specify values you want to change

# 3. Add script to your package.json
{
  "scripts": {
    "vagrant": "cd packages/vm && VM_CONFIG=../../vm.json vagrant"
  }
}

# 4. Start VM
yarn vagrant up      # Creates VM, installs everything

# 5. Enter VM
yarn vagrant ssh     # You're now in Ubuntu with Node.js + beautiful terminal!
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
yarn vagrant status   # Check if running
yarn vagrant halt     # Stop VM (preserves data)
yarn vagrant destroy  # Delete VM completely
yarn vagrant reload   # Restart VM with new config
yarn vagrant provision # Re-run Ansible provisioning
```

## 📁 File Sync

Edit anywhere - changes sync instantly:
```
Mac: ~/your-project/src/app.js
 ↕️
VM:  /workspace/src/app.js
```

## 🔌 Port Conflicts

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

## ✅ Requirements

- Vagrant 2.0+
- VirtualBox/Parallels/VMware
- 4GB free RAM