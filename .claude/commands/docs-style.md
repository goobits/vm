# Documentation Styling Prompt

## Instructions

Transform the provided documentation using this style guide:

1. **Structure** - Reorganize into the template below
2. **Emojis** - Add to headers per guidelines (sparingly, functionally)
3. **Descriptions** - Concise, precise, factual (no hype or sales language)
4. **Code** - Real, runnable examples with helpful comments
5. **Tone** - Professional, clear, understated elegance

**Goal**: Clean, scannable documentation that respects the reader's time.

## Styling Guidelines

### Emojis
- Headers: Always use relevant emoji + text
- Common patterns:
  - 🚀 Quick Start / Getting Started
  - ✨ Features / Highlights
  - 📚 Documentation / Library
  - 🛠️ Tools / Configuration / Setup
  - ⚙️ Settings / Config
  - 📖 Docs / Guides
  - 🔗 Links / Related
  - 🧪 Testing / Development
  - 💡 Help / Support
  - 📝 License / Legal
  - 🎯 Simple/Direct features
  - 🔧 Technical/Tool features
  - 🌐 Network/Global features
  - 🤖 AI/Bot/Automation features
  - ⚡ Performance/Speed features
  - 🔄 Sync/Update features

### Writing Style
- **Tone**: Professional, understated, precise
- **Descriptions**: Factual, concise, utility-focused (not promotional)
- **Commands**: Real, working examples
- **Comments**: Use # in code blocks sparingly for clarity
- **Lists**: Consistent bullet formatting
- **Bold**: Only for navigation and key terms

### Structure Rules
- Project name + one-line description (what it does, not why it's great)
- Features first, but factual not promotional
- Code examples over explanations
- Group related content logically
- Simple → Complex progression
- Minimal required setup upfront

### Code Blocks
- Always specify language for syntax highlighting
- Include helpful comments with #
- Show multiple ways to do things (CLI flags, env vars, config files)
- Real, runnable examples - not pseudo-code
- Show both simple and advanced usage

---

## TEMPLATE

```markdown
# [emoji] Project Name
[One-line description of what it does - factual, not promotional]

## ✨ Key Features
- **[emoji] Feature Name** - What it does
- **[emoji] Feature Name** - Specific capability
- [Use 4-6 features, focus on functionality not superlatives]

## 🚀 Quick Start
```bash
# Installation (keep it simple)
[primary install command]
[alternative install options if needed]

# Configuration (minimal required setup)
[essential config like API keys]
[show multiple options when available]

# Basic usage examples
[simplest possible command]
[pipe example if applicable]
[slightly more complex example]
```

## 📚 [Language/API] Library (if applicable)
```[language]
# Import statement
[clean import example]

# Basic usage
[simplest possible code example]

# Intermediate usage
[streaming or async example if relevant]

# Advanced feature
[sessions/context/state management example]
```

## 🛠️ [Advanced Feature Section] (optional)
```[language]
# Show practical examples
[real-world use case]

# Custom extensions
[how to extend/customize]
```

## ⚙️ Configuration (if configurable)
```bash
# View/list commands
[config viewing examples]

# Set/modify commands
[config setting examples]

# Shortcuts/aliases
[convenience features]
```

## 📖 Documentation
- **[Link Name](path)** - What you'll find there
- [Group related docs together]
- [Use descriptive names, not just "API Docs"]

## 🔗 Related Projects (optional)
- **[Project](url)** - Brief description
- [List ecosystem/companion projects]

## 🧪 Development (if open source)
```bash
# Dev setup
[developer installation]

# Testing
[test commands with descriptions]

# Code quality
[linting/formatting commands]
```

## 📝 License
[License type] - see [LICENSE](LICENSE) for details

## 💡 Support
- [Where to find help]
- [How to report issues]
```

---

## Optional Sections

Include these only if relevant to the project:
- Related Projects
- Development/Contributing
- Changelog
- Badges
- Screenshots/GIFs
- Performance benchmarks
- Migration guides
- FAQ

## Notes

- **Flexibility**: Include only relevant sections
- **Order**: Arrange by importance/logic for the project
- **Examples**: Prioritize code examples over descriptions
- **Aesthetic**: Clean, precise, understated - let the functionality speak for itself

$ARGUMENTS