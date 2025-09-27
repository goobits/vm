# Auth Proxy Service Proposal v2

## Overview
Replace the current `claude_sync`/`gemini_sync` approach with a centralized secrets management service for secure credential sharing.

## Current State
- ✅ `claude_sync: true` installs `@anthropic-ai/claude-code` npm package
- ✅ `gemini_sync: true` installs `@google/gemini-cli` npm package
- ❌ No centralized secret management
- ❌ Manual credential setup in each VM

## Problem
Credential management pain points:
```bash
# Current: Manual setup in every VM
vm ssh frontend
> export OPENAI_API_KEY=sk-xxxxx
vm ssh backend
> export OPENAI_API_KEY=sk-xxxxx  # Copy-paste same key
vm ssh api
> export OPENAI_API_KEY=sk-xxxxx  # Again...

# Key rotation = update every VM manually
# Team sharing = everyone needs their own keys
```

## Solution
Centralized auth service embedded in main VM binary with encrypted storage.

### Configuration
```yaml
# vm.yaml
auth_proxy: true  # Connect to host auth service

# VMs automatically get environment variables
```

### CLI Interface (Clean Break)
```bash
# Secrets management (replaces claude_sync/gemini_sync)
vm auth start                    # Start auth service on host (port 3090)
vm auth stop                     # Stop service
vm auth status                   # Show service status and secret count

vm auth add openai sk-xxxxx      # Store API key
vm auth list                     # Show stored keys (masked)
vm auth remove openai            # Delete key
vm auth rotate openai sk-yyyyy   # Update existing key
vm auth export my-app            # Get env file for VM

# NO claude_sync/gemini_sync - clean break to auth_proxy
```

### Implementation Tasks

1. **Create auth module** (`rust/vm/src/commands/auth/`):
   ```rust
   pub enum AuthCommand {
       Start { port: Option<u16> },
       Stop,
       Status,
       Add { name: String, value: String },
       List,
       Remove { name: String },
       Rotate { name: String, value: String },
       Export { vm_name: String },
   }
   ```

2. **Embedded HTTP service**:
   ```rust
   // Basic HTTP API for secret retrieval
   GET /secrets/{key_name}           # Get secret value
   POST /secrets/{key_name}          # Store secret
   DELETE /secrets/{key_name}        # Delete secret
   GET /env/{vm_name}               # Get all env vars for VM
   ```

3. **Encrypted storage** (`~/.vm/auth/secrets.json`):
   ```json
   {
     "secrets": {
       "openai": {
         "value": "encrypted_value_with_aes256gcm",
         "created": "2025-01-01T00:00:00Z",
         "scope": "global"
       }
     },
     "salt": "random_32_byte_salt",
     "version": 1
   }
   ```

4. **VM integration** (`rust/vm-provider/src/*/provisioning.rs`):
   - Check if `auth_proxy: true` in config
   - Fetch secrets from `http://host.docker.internal:3090/env/{vm_name}`
   - Create `/etc/environment.d/vm-auth.conf` with environment variables
   - Add systemd service to refresh secrets on boot

5. **Auto-start logic**:
   - `vm create` checks if auth service running on port 3090
   - If not running and `auth_proxy: true`, prompt: "Start auth service? [Y/n]"
   - Auto-execute `vm auth start` if user confirms

6. **Migration strategy (clean break)**:
   - Remove `claude_sync` and `gemini_sync` from schema
   - Provide one-time migration tool: `vm auth migrate-legacy`
   - Update all example configs to use `auth_proxy: true`

### Security Features
- AES-256-GCM encryption for stored secrets
- Key derivation using PBKDF2 with system-generated salt
- Bearer token authentication between VM and host
- Automatic key rotation detection
- Audit log for secret access

### Expected Results
- **One-time setup**: `vm auth add openai sk-xxxxx`
- **All VMs automatically get secrets**: No manual configuration
- **Key rotation**: Update once, affects all VMs instantly
- **Clean architecture**: No legacy sync mechanisms
- **Security**: Encrypted storage, controlled access

### Technical Notes
- Auth service embedded in main VM binary (port 3090)
- Secrets encrypted at rest using AES-256-GCM
- HTTP API with bearer token authentication between VM and host
- No backward compatibility with claude_sync/gemini_sync
- Service persists between VM lifecycles

### Success Criteria
- [ ] Auth service embedded in main VM binary (port 3090)
- [ ] Encrypted secret storage with AES-256-GCM
- [ ] VMs automatically receive environment variables when `auth_proxy: true`
- [ ] Migration tool from claude_sync/gemini_sync working
- [ ] Bearer token authentication between VM and host secure

### Breaking Changes
- **claude_sync: true** and **gemini_sync: true** no longer supported
- Must migrate to **auth_proxy: true** + **vm auth add** commands
- Existing VMs with sync enabled will show deprecation errors
- Migration tool: `vm auth migrate-legacy` provided