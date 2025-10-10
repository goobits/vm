# End-to-End Workflow Validation Proposal

## Status: 🎯 READY FOR TESTING

## Background

Setup validation (Phase 1) is complete. All critical blockers fixed:
- ✅ Build system validated
- ✅ Test suite (524 tests passing)
- ✅ Sudo Docker UID fix
- ✅ Test skip logic for Python/NPM
- ✅ Dockerfile permissions

Now we need to validate **real-world developer workflows** from cold boot.

---

## Objectives

Validate that a new developer can successfully:

1. **Install the tool** from scratch
2. **Create and use VMs** for actual development work
3. **Use core features** without hitting blockers
4. **Follow the happy path** through typical workflows

---

## Test Scenarios

### 1. 🚀 **Fresh Installation Test** (30 min)

**Goal**: Verify clean install on multiple platforms

**Platforms to test**:
- ✅ Linux (Ubuntu/Debian) with Docker requiring sudo
- ✅ Linux (Fedora/RHEL)
- ✅ macOS (Apple Silicon)
- ✅ macOS (Intel)

**Steps**:
```bash
# 1. Clone repo
git clone <repo-url>
cd vm

# 2. Install from source
./install.sh --build-from-source

# 3. Verify installation
vm --version
vm --help

# 4. Check PATH configuration
which vm
echo $PATH | grep .cargo/bin
```

**Success Criteria**:
- ✅ Install completes without errors
- ✅ `vm` command available in PATH
- ✅ `vm --version` shows correct version
- ✅ No sudo required (except for Docker operations)

---

### 2. 🐳 **VM Lifecycle Test** (20 min)

**Goal**: Validate basic VM operations work end-to-end

**Steps**:
```bash
# 1. Initialize project
mkdir test-project
cd test-project
vm init

# 2. Review generated config
cat vm.yaml

# 3. Create VM
vm create --force

# 4. Start VM
vm start

# 5. Check status
vm status
vm list

# 6. SSH into VM
vm ssh
# Inside VM:
pwd                    # Should be in /workspace
ls -la                 # Should see project files
rustc --version        # Check Rust available
node --version         # Check Node available
python3 --version      # Check Python available
exit

# 7. Stop VM
vm stop

# 8. Cleanup
vm destroy --force
```

**Success Criteria**:
- ✅ `vm init` creates valid vm.yaml
- ✅ `vm create` succeeds (no UID 0 error!)
- ✅ `vm start` brings VM online
- ✅ `vm ssh` connects successfully
- ✅ Language runtimes available in container
- ✅ Project files mounted correctly
- ✅ `vm destroy` cleans up completely

**Red Flags**:
- ❌ Permission denied errors
- ❌ Docker build failures
- ❌ Container won't start
- ❌ SSH connection refused

---

### 3. 📦 **Package Manager Test** (15 min)

**Goal**: Verify package installation works

**Steps**:
```bash
# 1. Create project with packages
mkdir pkg-test
cd pkg-test

# 2. Create vm.yaml with packages
cat > vm.yaml << 'EOF'
project:
  name: pkg-test

packages:
  cargo:
    - ripgrep
  npm:
    - prettier
  pip:
    - black
EOF

# 3. Create VM with packages
vm create --force
vm start

# 4. Verify packages installed
vm ssh -c "rg --version"
vm ssh -c "prettier --version"
vm ssh -c "black --version"

# 5. Cleanup
vm destroy --force
```

**Success Criteria**:
- ✅ Packages install during VM creation
- ✅ All package managers work (cargo, npm, pip)
- ✅ Binaries available in PATH
- ✅ No permission issues

---

### 4. 🔄 **Multi-Instance Test** (10 min)

**Goal**: Verify multiple VMs can coexist

**Steps**:
```bash
# 1. Create two projects
mkdir project-a project-b

# 2. Initialize both
cd project-a && vm init && cd ..
cd project-b && vm init && cd ..

# 3. Create both VMs
cd project-a && vm create --force && vm start && cd ..
cd project-b && vm create --force && vm start && cd ..

# 4. List all VMs
vm list

# 5. SSH into specific VM
cd project-a && vm ssh -c "echo 'Project A'" && cd ..
cd project-b && vm ssh -c "echo 'Project B'" && cd ..

# 6. Cleanup
cd project-a && vm destroy --force && cd ..
cd project-b && vm destroy --force && cd ..
```

**Success Criteria**:
- ✅ Both VMs create successfully
- ✅ Both VMs run simultaneously
- ✅ `vm list` shows both instances
- ✅ Can SSH into correct VM from each project dir
- ✅ No conflicts between instances

---

### 5. 🎨 **Framework Detection Test** (10 min)

**Goal**: Verify auto-detection of project types

**Test projects**:
```bash
# React project
npx create-react-app react-test
cd react-test
vm init  # Should detect React

# Rust project
cargo new rust-test
cd rust-test
vm init  # Should detect Rust

# Python project
mkdir python-test && cd python-test
echo "flask" > requirements.txt
vm init  # Should detect Python/Flask
```

**Success Criteria**:
- ✅ Framework detection works
- ✅ Appropriate presets suggested
- ✅ vm.yaml contains relevant packages/config
- ✅ Language-specific settings applied

---

### 6. 🔧 **Hot Reload / File Sync Test** (10 min)

**Goal**: Verify file changes sync between host and container

**Steps**:
```bash
# 1. Create project
mkdir sync-test && cd sync-test
vm init

# 2. Start VM
vm create --force && vm start

# 3. Create file on host
echo "console.log('hello')" > test.js

# 4. Verify file appears in container
vm ssh -c "cat /workspace/test.js"

# 5. Modify file on host
echo "console.log('modified')" > test.js

# 6. Verify change syncs
vm ssh -c "cat /workspace/test.js"

# 7. Create file in container
vm ssh -c "echo 'created in container' > container.txt"

# 8. Verify file appears on host
cat container.txt
```

**Success Criteria**:
- ✅ Host files visible in container immediately
- ✅ Host changes reflect in container
- ✅ Container changes reflect on host
- ✅ Correct file permissions preserved

---

### 7. 🌐 **Port Forwarding Test** (10 min)

**Goal**: Verify exposed ports work

**Steps**:
```bash
# 1. Create project with exposed port
cat > vm.yaml << 'EOF'
project:
  name: port-test

vm:
  ports:
    - "3000:3000"
EOF

# 2. Start VM
vm create --force && vm start

# 3. Start server in container
vm ssh -c "python3 -m http.server 3000 &"

# 4. Test from host
curl http://localhost:3000

# 5. Cleanup
vm destroy --force
```

**Success Criteria**:
- ✅ Port forwarding configured correctly
- ✅ Service accessible from host
- ✅ Correct port mapping in docker/compose

---

## Test Matrix

| Scenario | Linux+sudo | Linux | macOS ARM | macOS Intel |
|----------|-----------|--------|-----------|-------------|
| Fresh Install | ⏳ | ⏳ | ⏳ | ⏳ |
| VM Lifecycle | ⏳ | ⏳ | ⏳ | ⏳ |
| Packages | ⏳ | ⏳ | ⏳ | ⏳ |
| Multi-Instance | ⏳ | ⏳ | ⏳ | ⏳ |
| Framework Detect | ⏳ | ⏳ | ⏳ | ⏳ |
| File Sync | ⏳ | ⏳ | ⏳ | ⏳ |
| Port Forward | ⏳ | ⏳ | ⏳ | ⏳ |

Legend: ⏳ Not tested | ✅ Pass | ❌ Fail | ⚠️ Warning

---

## Expected Issues

Based on current codebase state:

1. **Clippy warnings** - Will cause `make check` to fail (known, low priority)
2. **Docker bake error** - May occur in some environments (investigate if reproducible)
3. **Platform-specific edge cases** - Need to document

---

## Success Criteria (Overall)

The project is **production-ready** when:

- ✅ All 7 scenarios pass on at least 2 platforms
- ✅ Critical path (install → create → ssh → destroy) is rock solid
- ✅ No HIGH severity issues blocking basic usage
- ✅ Documentation matches actual behavior

---

## Deliverable

Generate a comprehensive report: `E2E_VALIDATION_REPORT.md`

Include:
- Test results for each scenario
- Platform-specific findings
- Performance metrics (build times, startup times)
- Any new issues discovered
- Recommendations for next improvements

---

## Timeline

- **Per platform**: ~2 hours
- **All platforms**: ~8 hours
- **Report generation**: 1 hour
- **Total**: ~9 hours

---

## Next Steps After This

If E2E validation passes:
1. ✅ **Ready for beta testing** with real users
2. 📝 **Create user onboarding guide**
3. 🎥 **Record demo videos**
4. 🚀 **Prepare for v1.0 release**

If issues found:
1. ❌ **Triage and fix blockers**
2. 🔄 **Re-run validation**
3. 📊 **Update documentation**
