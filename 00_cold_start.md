VM: COLDSTART

Testing Instructions: Database Environment Variables Fix

Prerequisites

Docker must be running Fresh clone or clean state of the vm tool Test Scenario: Fresh Installation

This simulates the exact issue you reported - a fresh installation where DATABASE_URL wasn't available.

Step 1: Clean Slate (Simulate Fresh Install)

Remove existing global config to simulate fresh install rm -f ~/.vm/config.yaml

Create a test directory mkdir -p ~/test-db-fix cd ~/test-db-fix

Step 2: Enable PostgreSQL

This will auto-create ~/.vm/config.yaml with PostgreSQL enabled vm config set services.postgresql.enabled true

Verify the config was created cat ~/.vm/config.yaml

You should see: services: postgresql: enabled: true Step 3: Create a VM

Create a new VM vm create --force

Wait for it to finish (should take 2-3 minutes) Step 4: Verify DATABASE_URL is Available

Method 1: Using vm exec (quick check) vm exec printenv DATABASE_URL

Expected output: postgresql://postgres:postgres@172.17.0.1:5432/test-db-fix (or host.docker.internal if you're on macOS/Windows) Method 2: Check all database environment variables vm exec printenv | grep -E "DATABASE_URL|REDIS_URL|MONGODB_URL"

Expected output should include: DATABASE_URL=postgresql://postgres:postgres@ Method 3: Using vm ssh (interactive verification) vm ssh

Then inside the VM: echo $DATABASE_URL printenv | grep DATABASE exit

Step 5: Verify the PostgreSQL Container is Running

Check that the shared PostgreSQL container is running docker ps | grep postgres

Expected: You should see a container named 'vm-postgres-global'



