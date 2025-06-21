# Full Docker-Vagrant Parity Plan 🎯

This document outlines the remaining steps to achieve 100% parity between Docker and Vagrant providers.

## Current Status
- ✅ Core services (PostgreSQL, Redis, MongoDB, Node.js)
- ✅ Supervisor-based service management for Docker
- ✅ Container detection in Ansible playbook
- ⚠️ Some features untested
- ❌ Full test suite not passing

## Action Items for Full Parity

### 1. Fix Hostname Setting in Docker (5 mins)
```yaml
# In playbook.yml, replace the hostname task with:
- name: Set hostname in container
  command: hostname {{ project_hostname }}
  when: is_docker_container
```

### 2. Fix Locale Warnings (10 mins)
Add to Dockerfile:
```dockerfile
ENV LANG=en_US.UTF-8
ENV LC_ALL=en_US.UTF-8
ENV LANGUAGE=en_US:en
```

### 3. Test & Fix Remaining Services (20 mins)

#### a. Xvfb (Headless Browser)
- Already has supervisor config in playbook
- Just needs testing with `headless_browser.enabled: true`

#### b. Docker-in-Docker
- Socket already mounted in docker-compose
- Just needs testing with `docker.enabled: true`

#### c. Database Backup Restore
- Create test backup file
- Verify restore process works

### 4. Fix Redis Supervisor Config (10 mins)
Update the Ansible playbook to use the working Redis command:
```yaml
- name: Create Redis supervisor config (Docker)
  copy:
    dest: /etc/supervisor/conf.d/redis.conf
    content: |
      [program:redis]
      command=/usr/bin/redis-server --bind 127.0.0.1 ::1 --port {{ project_config.ports.redis | default(6379) }} --protected-mode yes
      autostart=true
      autorestart=true
      stderr_logfile=/var/log/supervisor/redis-err.log
      stdout_logfile=/var/log/supervisor/redis-out.log
  when: is_docker_container
```

### 5. Run Full Test Suite (15 mins)
```bash
# Create comprehensive test config
cat > test-full-parity.json << 'EOF'
{
  "provider": "docker",
  "project": {
    "name": "fullparitytest",
    "hostname": "parity.local",
    "workspace_path": "/workspace"
  },
  "ports": {
    "postgresql": 25432,
    "redis": 26379,
    "mongodb": 27017
  },
  "services": {
    "postgresql": { "enabled": true, "database": "testdb", "password": "testpass" },
    "redis": { "enabled": true },
    "mongodb": { "enabled": true },
    "docker": { "enabled": true },
    "headless_browser": { "enabled": true }
  },
  "npm_packages": ["@anthropic-ai/claude-code"],
  "terminal": { "emoji": "🧪", "username": "test", "theme": "dracula" }
}
EOF

# Run for both providers
./test-parity.sh --config test-full-parity.json
```

### 6. Fix Any Failing Tests (30 mins)
Based on test results:
- Update conditional logic in playbook
- Add Docker-specific workarounds where needed
- Ensure all 17 tests pass

### 7. Create Docker-Specific Handler for Service Restarts (10 mins)
Add proper supervisor handlers that work during provisioning:
```yaml
handlers:
  - name: restart postgresql docker
    supervisorctl:
      name: postgresql
      state: restarted
    when: is_docker_container

  - name: restart redis docker
    supervisorctl:
      name: redis
      state: restarted
    when: is_docker_container
```

### 8. Volume Permission Fix (5 mins)
Add to vm.sh docker_up function:
```bash
# Fix volume permissions before Ansible
docker_run "exec" "$config" "$project_dir" chown -R vagrant:vagrant /home/vagrant/.nvm /home/vagrant/.cache
```

### 9. Update Documentation (10 mins)
- Add Docker-specific notes to README.md
- Document any remaining differences
- Add troubleshooting section

## Total Time Estimate: ~2 hours

## Definition of "Full Parity"
- ✅ All 17 tests in test-parity.sh pass for both providers
- ✅ Same commands work in both environments
- ✅ Services accessible on same ports
- ✅ Development workflow identical
- ✅ Only documented, unavoidable differences (like systemd vs supervisor)

## Files to Remove (No Longer Needed)

### Test Files (created during debugging)
```bash
rm -f test-cleanup.sh
rm -f test-current-env.sh
rm -f test-docker.json
rm -f test-providers.sh
rm -f test-quick-parity.sh
rm -f test-running-env.sh
rm -f test-simple-parity.sh
rm -f test-vagrant-quick.json
rm -f test-vagrant.json
rm -f verify-parity.sh
```

### Generated Files (auto-created by vm.sh)
```bash
rm -f docker-compose.yml
```

### Obsolete Files
```bash
rm -f providers/docker/docker-compose.yml.template  # Using simple generator now
rm -f providers/docker/docker-provisioning.cjs     # Replaced by docker-provisioning-simple.cjs
```

### Keep These Essential Files
- ✅ `vm.sh` - main script
- ✅ `vm.json` - default config
- ✅ `vm.schema.json` - config schema
- ✅ `validate-config.js` - validation
- ✅ `test-parity.sh` - official test suite
- ✅ `install.sh` - installation
- ✅ `package.json` - npm package
- ✅ `README.md` - documentation
- ✅ `examples/` - example configs
- ✅ `shared/` - shared resources
- ✅ `providers/` - provider implementations

## Next Steps
1. Execute this plan step by step
2. Run full test suite after each major change
3. Document any new findings
4. Celebrate when all 17 tests pass! 🎉

---

## TODO Checklist - Parity Resolution Tasks

### Phase 1: Core Configuration Fixes (30 mins)
- [ ] **Fix hostname setting in Docker** (5 mins)
  - [ ] Update playbook.yml with container-specific hostname command
  - [ ] Test hostname command works in Docker container
- [ ] **Add locale environment variables** (10 mins)
  - [ ] Update Dockerfile with LANG=en_US.UTF-8
  - [ ] Update Dockerfile with LC_ALL=en_US.UTF-8  
  - [ ] Update Dockerfile with LANGUAGE=en_US:en
  - [ ] Test locale warnings are resolved
- [ ] **Fix Redis supervisor configuration** (10 mins)
  - [ ] Update Ansible playbook with correct Redis bind/port command
  - [ ] Test Redis starts properly with supervisor in Docker
- [ ] **Add volume permission fixes** (5 mins)
  - [ ] Update vm.sh docker_up function to fix NVM permissions
  - [ ] Update vm.sh docker_up function to fix cache permissions
  - [ ] Test Node.js/npm work without permission errors

### Phase 2: Service Management (20 mins)
- [ ] **Add Docker-specific service restart handlers** (10 mins)
  - [ ] Create supervisorctl handler for PostgreSQL restart
  - [ ] Create supervisorctl handler for Redis restart
  - [ ] Test handlers work during Ansible provisioning
- [ ] **Test additional services** (10 mins)
  - [ ] Test Xvfb (headless browser) service with Docker
  - [ ] Test Docker-in-Docker functionality
  - [ ] Test database backup/restore functionality

### Phase 3: Testing & Validation (60 mins)
- [ ] **Run comprehensive test suite** (15 mins)
  - [ ] Execute test-parity.sh for Vagrant provider
  - [ ] Execute test-parity.sh for Docker provider
  - [ ] Document specific test failures
- [ ] **Fix failing tests** (30 mins)
  - [ ] Fix basic functionality tests (user, pwd, hostname, workspace)
  - [ ] Fix development tools tests (Node.js, npm, pnpm, Git, Zsh)
  - [ ] Fix service connectivity tests (PostgreSQL, Redis, MongoDB)
  - [ ] Fix terminal customization tests (emoji, username)
  - [ ] Fix environment tests (locale, TERM variable)
  - [ ] Fix file synchronization tests
- [ ] **Verify all 17 tests pass** (15 mins)
  - [ ] Confirm 100% test pass rate for Docker
  - [ ] Confirm 100% test pass rate for Vagrant
  - [ ] Confirm parity comparison tests all pass

### Phase 4: Documentation & Cleanup (10 mins)
- [ ] **Clean up temporary test files** (5 mins)
  - [ ] Remove test-cleanup.sh, test-docker.json, test-vagrant.json
  - [ ] Remove other temporary test files listed above
  - [ ] Keep only essential test-parity.sh
- [ ] **Update documentation** (5 mins)
  - [ ] Mark all tasks in this checklist as completed
  - [ ] Update README.md with Docker-specific notes if needed
  - [ ] Document any remaining unavoidable differences

### Success Criteria
- [ ] **All 17 tests in test-parity.sh pass for both providers**
- [ ] **Same commands work identically in both environments**
- [ ] **Services accessible on same ports with same functionality**
- [ ] **Development workflow identical between providers**
- [ ] **Only documented, unavoidable differences remain (systemd vs supervisor)**

**Total Estimated Time**: ~2 hours  
**Current Status**: Ready to execute  
**Expected Outcome**: Full Docker-Vagrant parity achieved 🎯