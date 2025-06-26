# VM Aliases Mechanism

## How Aliases Work

Aliases from `vm.json` are applied to the VM through the Ansible provisioning process. Here's the exact mechanism:

### 1. Configuration Loading
- When the VM is provisioned, the `vm.json` configuration is loaded and passed to Ansible
- The config is saved to `/tmp/vm-config.json` inside the VM

### 2. Alias Processing (Ansible Playbook)
- The Ansible playbook (lines 729-751 in `playbook.yml`) processes aliases:
  ```yaml
  - name: Generate project aliases
    set_fact:
      project_aliases_block: |
        # Project aliases (from infrastructure.json)
        {% for alias_name, alias_command in (project_config.aliases | default({})).items() %}
        alias {{ alias_name }}='{{ alias_command }}'
        {% endfor %}
  ```

### 3. Writing to Shell Config
- Aliases are written to `/home/vagrant/.zshrc` using Ansible's `blockinfile` module
- They're placed between markers: `# BEGIN ANSIBLE MANAGED - PROJECT` and `# END ANSIBLE MANAGED - PROJECT`

## Applying Aliases Without Full Reprovisioning

Currently, there are **three ways** to apply alias changes:

### Method 1: Manual Update (Quickest)
```bash
# 1. SSH into the VM
vm ssh

# 2. Edit .zshrc directly
nano ~/.zshrc

# 3. Find the section between:
# BEGIN ANSIBLE MANAGED - PROJECT
# END ANSIBLE MANAGED - PROJECT

# 4. Update your aliases manually to match vm.json

# 5. Reload the shell
source ~/.zshrc
```

### Method 2: Partial Ansible Run (Docker)
For Docker environments, you can run just the alias-related Ansible tasks:

```bash
# 1. Copy updated config to container
docker cp vm.json $(docker ps -qf "name=-dev"):/tmp/vm-config.json

# 2. Run specific Ansible tasks
vm exec ansible-playbook -i localhost, -c local \
  /vm-tool/providers/vagrant/ansible/playbook.yml \
  --tags "project-aliases" \
  --start-at-task "Generate project aliases"
```

### Method 3: Full Reprovision (Safest)
```bash
vm provision
```

## Why Aliases Require Provisioning

The current architecture doesn't include a lightweight "update aliases only" command because:

1. **Unified Architecture**: Both Docker and Vagrant use the same Ansible playbook
2. **Idempotency**: Ansible ensures consistent state across provisions
3. **Dependencies**: Aliases might depend on other configuration (ports, services)

## Example: Adding the claudeyolo Alias

Your `vm.json` contains:
```json
"aliases": {
    "claudeyolo": "claude --dangerously-bypass-permissions"
}
```

This gets translated by Ansible into:
```bash
alias claudeyolo='claude --dangerously-bypass-permissions'
```

And written to `~/.zshrc` in the VM.

## Future Enhancement Possibility

A dedicated alias update command could be added to `vm.sh`:
```bash
vm update-aliases
```

This would:
1. Copy the latest vm.json to the VM
2. Run only the alias-related Ansible tasks
3. Reload the shell configuration

However, this would need to be implemented for both Docker and Vagrant providers.