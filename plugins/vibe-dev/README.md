# Vibe Development Plugin

Enhanced workflow environment with AI assistant integration.

## What's Included

### System Packages
- `tree` - Directory tree viewer
- `ripgrep` - Fast search tool
- `unzip` - Archive extraction
- `htop` - Process monitor

### NPM Packages
- `@anthropic-ai/claude-code` - Claude AI integration
- `@google/gemini-cli` - Gemini AI CLI
- `@openai/codex` - OpenAI Codex integration

### Python Packages
- `claudeflow` - Claude workflow tools

### Aliases
- `claudeyolo` → `claude --dangerously-skip-permissions`
- `geminiyolo` → `GEMINI_API_KEY=${GEMINI_API_KEY:-} gemini --approval-mode=yolo`
- `codexyolo` → `codex --dangerously-bypass-approvals-and-sandbox`

## Installation

```bash
vm plugin install plugins/vibe-dev
```

## Usage

```bash
vm config preset vibe
```

## License

MIT