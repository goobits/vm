# Proposal: Cold Start Test for PostgreSQL Fix

## Problem

We need a comprehensive test plan to validate that the PostgreSQL service health check timeout fix (01a) works correctly in real-world scenarios, particularly for fresh installations where DATABASE_URL environment variables must be available immediately.

## Objective

Create and execute a cold start test that simulates a fresh installation to verify:
1. PostgreSQL service starts successfully with new timeout values
2. DATABASE_URL is correctly injected into VMs
3. No hanging or timeout issues during VM creation

## Prerequisites

- [ ] Docker must be running
- [ ] Fresh clone or clean state of the vm tool
- [ ] PostgreSQL fix (01a) has been implemented

## Test Scenario: Fresh Installation

This simulates the exact issue reported - a fresh installation where DATABASE_URL wasn't available.

### Checklist

#### Step 1: Clean Slate
- [ ] Remove existing global config to simulate fresh install: `rm -f ~/.vm/config.yaml`
- [ ] Create a test directory: `mkdir -p ~/test-db-fix && cd ~/test-db-fix`

#### Step 2: Enable PostgreSQL
- [ ] Auto-create config with PostgreSQL enabled: `vm config set services.postgresql.enabled true`
- [ ] Verify the config was created: `cat ~/.vm/config.yaml`
- [ ] Confirm output shows:
  ```yaml
  services:
    postgresql:
      enabled: true
  ```

#### Step 3: Create a VM
- [ ] Create a new VM: `vm create --force`
- [ ] Wait for completion (should take 2-3 minutes max, not hang)
- [ ] Verify no timeout errors occur

#### Step 4: Verify DATABASE_URL is Available

**Method 1: Using vm exec (quick check)**
- [ ] Run: `vm exec printenv DATABASE_URL`
- [ ] Expected output: `postgresql://postgres:postgres@172.17.0.1:5432/test-db-fix`
  - (or `host.docker.internal` on macOS/Windows)

**Method 2: Check all database environment variables**
- [ ] Run: `vm exec printenv | grep -E "DATABASE_URL|REDIS_URL|MONGODB_URL"`
- [ ] Expected: `DATABASE_URL=postgresql://postgres:postgres@...` present

**Method 3: Using vm ssh (interactive verification)**
- [ ] Run: `vm ssh`
- [ ] Inside VM, run: `echo $DATABASE_URL`
- [ ] Inside VM, run: `printenv | grep DATABASE`
- [ ] Exit: `exit`
- [ ] Verify DATABASE_URL was displayed

#### Step 5: Verify the PostgreSQL Container is Running
- [ ] Check container is running: `docker ps | grep postgres`
- [ ] Expected: Container named 'vm-postgres-global' is present
- [ ] Test connection: `docker exec vm-postgres-global pg_isready`

## Success Criteria

- [ ] VM creation completes without hanging
- [ ] DATABASE_URL is available inside the VM
- [ ] PostgreSQL container is running and accepting connections
- [ ] Entire process takes < 3 minutes
- [ ] No error messages or timeouts

## Related Proposals

- **01a_bug_postgresql_startup_hang.md** - The fix being validated by this test
