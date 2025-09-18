# Git Hooks Setup and Usage

This document describes the Git hooks configuration for the VM project, which automate code quality checks and testing.

## 🎯 Overview

The VM project uses Git hooks to maintain code quality and prevent issues from entering the repository. The hooks are designed to be fast, informative, and easy to bypass when needed.

## 🔧 Installed Hooks

### 1. Pre-commit Hook (`pre-commit`)

**Purpose:** Runs fast quality checks before allowing commits

**What it does:**
- ✅ Checks Rust code formatting with `rustfmt`
- ✅ Runs Clippy linting with warnings as errors
- ✅ Performs quick dead code detection
- ✅ Runs unit tests for affected packages

**When it runs:** Before every `git commit`

**Typical runtime:** 30-60 seconds

### 2. Pre-push Hook (`pre-push`)

**Purpose:** Runs comprehensive testing before allowing pushes

**What it does:**
- ✅ Runs the full test suite (`cargo test --workspace`)
- ✅ Runs documentation tests (`cargo test --doc`)
- ✅ Performs comprehensive dead code analysis
- ✅ Checks for unused dependencies (if `cargo-machete` is installed)
- ✅ Verifies version synchronization
- ✅ Tests release build compilation

**When it runs:** Before every `git push`

**Typical runtime:** 3-10 minutes

### 3. Commit Message Hook (`commit-msg`)

**Purpose:** Validates commit message format and quality

**What it does:**
- ✅ Enforces conventional commit format (`type(scope): description`)
- ✅ Validates message length and content
- ✅ Checks for descriptive commit messages
- ✅ Warns about breaking changes

**When it runs:** Before every `git commit`

**Typical runtime:** < 1 second

## 🚀 Setup Instructions

### Automatic Setup (Recommended)

The hooks are already installed and configured when you clone this repository. They should work immediately if you have the Rust toolchain installed.

### Manual Setup

If the hooks aren't working or you need to reinstall them:

```bash
# Make sure hooks are executable
chmod +x .git/hooks/pre-commit
chmod +x .git/hooks/pre-push
chmod +x .git/hooks/commit-msg

# Test the pre-commit hook
.git/hooks/pre-commit

# Test commit message validation
echo "feat: test commit message" | .git/hooks/commit-msg /dev/stdin
```

### Prerequisites

Ensure you have the required tools installed:

```bash
# Rust toolchain (required)
source $HOME/.cargo/env

# Optional: Install cargo-machete for dependency analysis
cargo install cargo-machete

# Optional: Install faster linker (see rust/.cargo/config.toml)
# Linux: apt install lld
# macOS: included with Xcode
```

## 📝 Usage Examples

### Normal Development Workflow

```bash
# Make changes to code
git add .

# Commit (triggers pre-commit and commit-msg hooks)
git commit -m "feat(vm-config): add port range validation"

# Push (triggers pre-push hook)
git push origin feature-branch
```

### Bypassing Hooks (When Needed)

```bash
# Skip pre-commit and commit-msg hooks
git commit --no-verify -m "wip: experimental changes"

# Skip pre-push hook
git push --no-verify origin feature-branch
```

### Testing Hooks Manually

```bash
# Test pre-commit hook
.git/hooks/pre-commit

# Test pre-push hook
.git/hooks/pre-push origin https://github.com/user/repo.git

# Test commit message validation
echo "your commit message" | .git/hooks/commit-msg /dev/stdin
```

## ✅ Commit Message Format

The commit-msg hook enforces conventional commit format:

### Valid Formats

```bash
# Basic format
git commit -m "type: description"

# With scope
git commit -m "type(scope): description"

# Examples
git commit -m "feat: add VM configuration validation"
git commit -m "fix(vm-provider): resolve Docker container lifecycle issue"
git commit -m "docs: update installation guide"
git commit -m "test(vm-config): add integration tests for port allocation"
```

### Valid Types

- `feat`: New features
- `fix`: Bug fixes
- `docs`: Documentation changes
- `style`: Code style changes (formatting, etc.)
- `refactor`: Code refactoring
- `test`: Adding or updating tests
- `chore`: Maintenance tasks
- `perf`: Performance improvements
- `ci`: CI/CD changes
- `build`: Build system changes

### Commit Message Guidelines

- **Subject line:** 10-72 characters
- **Start with lowercase:** `feat: add feature` not `feat: Add feature`
- **No period at end:** `feat: add feature` not `feat: add feature.`
- **Be descriptive:** `fix: resolve port allocation race condition` not `fix: bug`

## 🛠️ Troubleshooting

### Hook Failures

**Pre-commit fails on formatting:**
```bash
cd rust
cargo fmt --all
git add .
git commit -m "style: format code"
```

**Pre-commit fails on Clippy warnings:**
```bash
cd rust
cargo clippy --workspace --all-targets --fix
git add .
git commit -m "fix: resolve clippy warnings"
```

**Pre-push fails on tests:**
```bash
cd rust
cargo test --workspace -- --nocapture
# Fix failing tests, then:
git commit -m "fix: resolve test failures"
```

### Performance Issues

**Pre-commit is slow:**
- The hook only runs tests for affected packages
- Consider using `git commit --no-verify` for work-in-progress commits
- Run `cargo clean` if incremental builds are corrupted

**Pre-push is very slow:**
- This is expected - it runs the full test suite
- Use `git push --no-verify` for experimental branches
- Consider running tests manually first: `cd rust && cargo test --workspace`

### Disabling Hooks

**Temporarily disable all hooks:**
```bash
# For current commit only
git commit --no-verify

# For current push only
git push --no-verify
```

**Permanently disable hooks:**
```bash
# Remove executable permission
chmod -x .git/hooks/pre-commit
chmod -x .git/hooks/pre-push
chmod -x .git/hooks/commit-msg

# Or rename them
mv .git/hooks/pre-commit .git/hooks/pre-commit.disabled
```

**Re-enable hooks:**
```bash
chmod +x .git/hooks/pre-commit
chmod +x .git/hooks/pre-push
chmod +x .git/hooks/commit-msg
```

## 🔍 Customization

### Modifying Hook Behavior

The hooks are shell scripts located in `.git/hooks/` and can be modified:

- `.git/hooks/pre-commit` - Fast quality checks
- `.git/hooks/pre-push` - Comprehensive testing
- `.git/hooks/commit-msg` - Message validation

### Adding New Checks

To add new checks to the pre-commit hook:

1. Edit `.git/hooks/pre-commit`
2. Add your check in the appropriate section
3. Follow the existing error handling patterns
4. Test the hook manually

### Environment Variables

The hooks respect these environment variables:

```bash
# Skip specific checks (example)
SKIP_CLIPPY=1 git commit -m "wip: experimental code"

# Rust environment
source $HOME/.cargo/env
```

## 📊 Performance Characteristics

| Hook | Typical Runtime | What it Tests |
|------|----------------|---------------|
| `commit-msg` | < 1 second | Message format only |
| `pre-commit` | 30-60 seconds | Format, lint, quick tests |
| `pre-push` | 3-10 minutes | Full test suite |

## 🎯 Best Practices

### For Developers

1. **Run tests locally** before committing to catch issues early
2. **Use descriptive commit messages** that explain the "why"
3. **Commit frequently** with small, logical changes
4. **Use `--no-verify` sparingly** and only for WIP commits

### For Code Reviews

1. **Hooks don't replace code review** - they catch basic issues
2. **Focus reviews on logic and design** since formatting/linting is automated
3. **Check that commit messages are meaningful** beyond just format

### For CI/CD

1. **Hooks complement CI** but don't replace it
2. **CI should run the same checks** for consistency
3. **Consider different hook configurations** for different branches

## 📞 Support

If you encounter issues with the Git hooks:

1. Check the troubleshooting section above
2. Verify your Rust toolchain is properly installed
3. Look at the hook output for specific error messages
4. Consider temporarily bypassing hooks with `--no-verify`
5. Report persistent issues to the development team

The hooks are designed to help maintain code quality while staying out of your way during normal development.