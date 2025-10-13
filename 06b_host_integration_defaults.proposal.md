# Host System Integration Improvements

**Status:** Open
**Impact:** Better integration between host and VM environments

---

## Problem

VMs don't automatically inherit useful host system configuration:

1. **Git Configuration Missing**: No git user.name or user.email configured, requiring manual setup on every VM
2. **Timezone Hardcoded**: Defaults to `America/Los_Angeles`, wrong for most users globally
3. **Essential Environment Variables Missing**: Standard variables like `EDITOR`, `VISUAL`, `PAGER` not set

These issues force developers to repeatedly configure the same settings in every VM.

---

## Proposed Solutions

### Solution 1: Auto-Copy Git Configuration from Host

**Implementation approach:**

**Option A: Copy on VM Creation**
- Read host's `~/.gitconfig` during `vm create`
- Copy `user.name`, `user.email`, `pull.rebase`, `init.defaultBranch` to VM
- Set in global git config inside VM

**Option B: Interactive Prompt on First SSH**
- Detect missing git config on first `vm ssh`
- Prompt: "Git not configured. Use host settings? (user: John Doe, email: john@example.com) [Y/n]"
- Apply if accepted

**Recommended: Option A** (seamless, no interruption)

**Fallback**: If host git not configured, prompt on first commit attempt

### Solution 2: Auto-Detect Timezone

**Implementation approach:**

Add `auto` option for timezone:
```yaml
# configs/defaults.yaml
vm:
  timezone: auto  # Instead of America/Los_Angeles
```

**Detection methods (in order of preference):**
1. Read from `TZ` environment variable
2. Read `/etc/timezone` on Linux host
3. Run `date +%Z` command
4. Parse `systemctl show --property=Timezone` on systemd systems
5. Fallback to `UTC` if detection fails

### Solution 3: Set Essential Environment Variables

**Add to container environment by default:**
```bash
EDITOR=vim
VISUAL=vim
PAGER="less -R"
TERM=xterm-256color
HISTSIZE=50000
SAVEHIST=50000
HISTFILE=~/.zsh_history
```

**Implementation locations:**
- Dockerfile.j2 (ENV directives)
- `.zshrc` additions
- Docker compose environment section

---

## Implementation Checklist

### Git Configuration Auto-Copy
- [ ] Add git config parsing utility in `rust/vm-config/`
- [ ] Read host's `~/.gitconfig` during VM creation
- [ ] Extract `user.name`, `user.email`, `pull.rebase`, `init.defaultBranch`
- [ ] Add to Ansible provisioning playbook
- [ ] Set git config in VM during provisioning
- [ ] Handle missing host git config gracefully
- [ ] Test with configured host git
- [ ] Test with unconfigured host git
- [ ] Add option to disable: `copy_git_config: false`
- [ ] Document in user guide

### Auto-Detect Timezone
- [ ] Add timezone detection logic to `rust/vm-config/`
- [ ] Implement detection methods (TZ env, /etc/timezone, date command)
- [ ] Add `auto` as valid timezone value
- [ ] Update `configs/defaults.yaml` to use `timezone: auto`
- [ ] Pass detected timezone to Docker container
- [ ] Update Dockerfile to accept TZ argument
- [ ] Test on Linux (various distributions)
- [ ] Test on macOS
- [ ] Test on Windows/WSL
- [ ] Test fallback to UTC when detection fails
- [ ] Document manual override in config

### Essential Environment Variables
- [ ] Add default env vars to Dockerfile.j2
- [ ] Set EDITOR, VISUAL, PAGER in container ENV
- [ ] Set TERM to xterm-256color
- [ ] Configure shell history size in .zshrc
- [ ] Add HISTFILE location for persistence
- [ ] Test env vars are set in new VMs
- [ ] Test env vars persist across restarts
- [ ] Document how to override defaults

### Git Config Sections to Copy
- [ ] `user.name`
- [ ] `user.email`
- [ ] `pull.rebase` (if set)
- [ ] `init.defaultBranch` (if set)
- [ ] `core.editor` → map to VM editor
- [ ] `core.excludesfile` → copy global gitignore if exists
- [ ] Consider: `alias.*` (git aliases)
- [ ] Consider: `core.autocrlf` (line endings)

---

## Success Criteria

### Git Configuration
- [ ] New VMs have git user.name and user.email pre-configured
- [ ] Git config matches host system settings
- [ ] First git commit works without prompting for identity
- [ ] `git config --list` shows copied configuration
- [ ] Graceful handling when host has no git config
- [ ] Option to disable auto-copy via config flag

### Timezone Detection
- [ ] VMs use host system timezone by default
- [ ] `date` command in VM shows correct local time
- [ ] Container TZ environment variable is set correctly
- [ ] Fallback to UTC works when detection fails
- [ ] Users can override with explicit timezone in config
- [ ] Works across Linux, macOS, and Windows/WSL

### Environment Variables
- [ ] `echo $EDITOR` returns `vim`
- [ ] `echo $VISUAL` returns `vim`
- [ ] `echo $PAGER` returns `less -R`
- [ ] `echo $TERM` returns `xterm-256color`
- [ ] Shell history size is 50000 lines
- [ ] History persists across sessions

---

## Alternative Approaches Considered

### Git Config via Volume Mount
**Approach:** Mount host `~/.gitconfig` read-only into container
**Pros:** Always in sync with host
**Cons:**
- Breaks if host paths don't exist in container
- Git aliases might reference host-only tools
- Less isolation between host and VM

**Verdict:** Copy approach is better

### Timezone via TZ Environment Variable Only
**Approach:** Just pass TZ env var, don't set container timezone
**Pros:** Simpler
**Cons:**
- Not all programs respect TZ
- System logs show wrong time
- Confusing output from `date` command

**Verdict:** Proper container timezone configuration is better

### Interactive Git Config Wizard
**Approach:** Run wizard on first `vm ssh` to configure git
**Pros:** User control
**Cons:**
- Interrupts workflow
- Annoying for experienced users
- Requires interactive terminal

**Verdict:** Auto-copy with opt-out is better

---

## Files to Modify

### Git Configuration
- `rust/vm-config/src/lib.rs` - Add git config parser
- `rust/vm-provider/src/docker/build.rs` - Pass git config to container
- `rust/vm-provider/src/resources/ansible/playbook.yml` - Apply git config
- `configs/defaults.yaml` - Add `copy_git_config: true` option

### Timezone Detection
- `rust/vm-config/src/lib.rs` - Add timezone detection
- `rust/vm-provider/src/docker/Dockerfile.j2` - Accept TZ build arg
- `rust/vm-provider/src/docker/build.rs` - Pass detected timezone
- `configs/defaults.yaml` - Change default to `timezone: auto`

### Environment Variables
- `rust/vm-provider/src/docker/Dockerfile.j2` - Add ENV directives
- `rust/vm-provider/src/docker/compose.rs` - Add to environment section
- Template `.zshrc` modifications for HISTSIZE

---

## Benefits

**Seamless Experience:**
- VMs "just work" without manual configuration
- Correct time display from day one
- Git commits work immediately without setup

**Time Savings:**
- No more configuring git in every VM
- No more setting timezone manually
- No more setting EDITOR/VISUAL in each shell

**Consistency:**
- Same git identity across host and all VMs
- Same timezone across all environments
- Predictable environment variables

**Fewer Surprises:**
- Time-based operations (cron, logs) show correct local time
- Git commits have correct author information
- Editors open correctly when invoked by other tools
