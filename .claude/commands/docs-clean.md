# 📚 Documentation Sync & Accuracy Guide

## Core Objective
Ensure all markdown files in the root directory represent the current codebase properly and factually - NO content bloat, just accuracy.

## Documentation Philosophy
- **Accuracy First**: Every statement must reflect current code reality
- **Concise & Dense**: Maximum information density, minimum word count
- **Right-sized**: Match detail level to document purpose (README = overview, API docs = specifics)
- **Single Source of Truth**: One canonical location per topic

## Systematic Review Process

### 1. Discovery Phase
- [ ] Scan all markdown files in root directory
- [ ] Map each doc to corresponding code areas
- [ ] Note any code areas lacking documentation

### 2. Verification Checklist
For each markdown file, verify:
- [ ] **API accuracy** - Function names, parameters, return types match code
- [ ] **Configuration** - Environment vars, config files, CLI flags are current
- [ ] **Installation** - Dependencies, setup commands, requirements still valid
- [ ] **Usage examples** - Code snippets actually run without errors
- [ ] **Architecture** - File structure and component relationships are correct
- [ ] **Features** - What's documented matches what's implemented

### 3. Update Guidelines

#### ✅ DO
- Fix incorrect information immediately
- Remove obsolete/deprecated content
- Add only missing critical information
- Maintain existing style and tone
- Use bullet points over paragraphs
- Test all code examples

#### ❌ DON'T
- Add verbose explanations
- Include implementation details in user docs
- Duplicate information across files
- Expand content unnecessarily
- Document internal/private APIs
- Modify proposals in any way

## Essential Coverage Areas
Ensure documentation covers (if applicable):
- Installation and setup
- Basic usage examples
- Configuration options
- Public API/CLI reference
- High-level architecture
- Troubleshooting guide

## Update Evidence
When making changes, note:
- **What**: Specific change made
- **Why**: Code reference that necessitated it
- **Impact**: How this affects users

## Final Checks
1. All code references are accurate
2. Code snippets execute successfully
3. Internal and external links work
4. Formatting is consistent

---

**Remember**: The goal is trustworthy, maintainable documentation that stays in sync with the codebase. When in doubt, verify against the code.

$ARGUMENTS