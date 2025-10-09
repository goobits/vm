# Vibe Development Plugin

Enhanced workflow environment with AI assistant integration and modern shell tools.

## What's Included

### System Packages
- `tree` - A recursive directory listing command that produces a depth-indented listing of files.
- `ripgrep` - A line-oriented search tool that recursively searches the current directory for a regex pattern.
- `unzip` - A utility for extracting and viewing files in ZIP archives.
- `htop` - An interactive process viewer.

### NPM Packages
- `@anthropic-ai/claude-code` - Command-line interface for Anthropic's Claude.
- `@google/gemini-cli` - Command-line interface for Google's Gemini.
- `@openai/codex` - Command-line interface for OpenAI's Codex.

### Python Packages
- `anthropic` - The official Python SDK for the Anthropic (Claude) API.

### Environment Variables
- This preset does not set any specific environment variables by default.

## Installation

This plugin is automatically installed with the VM tool. No additional installation required.

To verify availability:
```bash
vm config preset --list | grep vibe
```

## Usage

Apply this preset to your project:
```bash
vm config preset vibe
vm create
```

Or add to `vm.yaml`:
```yaml
preset: vibe
```

## Configuration

### Additional Packages
```yaml
preset: vibe
packages:
  npm:
    - some-other-ai-tool
  pip:
    - openai
```

## Common Use Cases

1. **Interacting with an AI model**
   ```bash
   # Ensure your API key is set as an environment variable
   export ANTHROPIC_API_KEY="your-key"
   vm exec "claude-code 'Translate this to French: Hello, world!'"
   ```

2. **Searching for text in your codebase**
   ```bash
   vm exec "rg 'my_function_name'"
   ```

## Troubleshooting

### Issue: AI tool reports an authentication error
**Solution**: Ensure you have set the required API key as an environment variable in your host shell or in the `vm.yaml` file. Common variables include `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`, and `OPENAI_API_KEY`.

### Issue: `command not found` for an installed tool
**Solution**: The tool might be installed in a location not in your `PATH`. Check the installation location and add it to your `PATH` in your `.bashrc` or `.zshrc` file within the VM.

## Related Documentation

- [Configuration Guide](../../docs/user-guide/configuration.md)
- [Presets Overview](../../docs/user-guide/presets.md)
- [CLI Reference](../../docs/user-guide/cli-reference.md)

## License

MIT