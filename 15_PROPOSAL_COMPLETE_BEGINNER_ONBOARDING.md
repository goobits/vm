# Proposal: Complete Beginner Onboarding Test

**Status:** 🧪 Onboarding Test - Path 1
**Priority:** P0
**Complexity:** Low (Testing Only)
**Estimated Effort:** 30-60 minutes
**Test Type:** Cold Start - Zero Prerequisites

---

## Purpose

This proposal tests the **complete beginner onboarding experience** for developers who have:
- ❌ No Rust toolchain installed
- ❌ No Docker installed
- ❌ No prior knowledge of the VM tool
- ✅ Basic terminal/command line knowledge
- ✅ Ability to follow README instructions

**Goal:** Measure how easy it is to go from zero to a working development environment by following only the public documentation.

---

## Test Persona

**Name:** Alex (Junior Developer)
**Background:**
- Just graduated from bootcamp
- Familiar with Git and basic CLI commands
- Has never used Rust or Docker
- Looking to quickly set up a development environment for a Node.js project

**Pain Points to Test:**
- Is the installation process clear?
- Are prerequisites well-documented?
- Do error messages help when things go wrong?
- Is the "happy path" actually happy?

---

## Test Environment

**Starting State:**
```bash
# Clean machine (VM, container, or fresh user account)
which cargo      # Should return: not found
which docker     # Should return: not found
which vm         # Should return: not found
```

**Test Machine Requirements:**
- Ubuntu 22.04 / macOS 13+ / Windows 10+ with WSL2
- Internet connection
- Sudo/admin access
- At least 4GB free disk space

---

## Test Procedure

### Phase 1: Discovery (5 minutes)

**Objective:** Can the tester find the project and understand what it does?

1. **Find the repository**
   - [ ] Navigate to project URL (provide GitHub link)
   - [ ] Read the README.md

2. **Record first impressions:**
   - Time to understand what the tool does: _____ minutes
   - Is the purpose clear? (Yes/No): _____
   - Is the value proposition compelling? (Yes/No): _____
   - Are there any confusing terms or jargon? (List): _____

### Phase 2: Prerequisites (10-20 minutes)

**Objective:** Can the tester install all required dependencies?

3. **Install Rust toolchain**
   - [ ] Follow instructions at https://rustup.rs or from README
   - [ ] Verify installation: `cargo --version`
   - **Record:**
     - Time to complete: _____ minutes
     - Any errors encountered: _____
     - Clarity of instructions (1-5): _____

4. **Install Docker**
   - [ ] Follow Docker installation for your OS
   - [ ] Verify installation: `docker --version`
   - [ ] Start Docker daemon: `docker ps`
   - **Record:**
     - Time to complete: _____ minutes
     - Any errors encountered: _____
     - Were Docker installation instructions clear in README? (Yes/No): _____

### Phase 3: VM Tool Installation (10-15 minutes)

**Objective:** Can the tester install the VM tool successfully?

5. **Install from Cargo** (Recommended path)
   ```bash
   cargo install vm
   ```
   - **Record:**
     - Installation started: [timestamp]
     - Installation completed: [timestamp]
     - Total time: _____ minutes
     - Compilation successful? (Yes/No): _____
     - Any warnings/errors: _____

6. **Verify installation**
   ```bash
   vm --version
   vm --help
   ```
   - **Record:**
     - Version installed: _____
     - Help text clear and useful? (1-5): _____

### Phase 4: First VM Creation (15-20 minutes)

**Objective:** Can the tester create their first VM without errors?

7. **Create a test project**
   ```bash
   mkdir ~/test-nodejs-project
   cd ~/test-nodejs-project
   echo '{"name": "test-app", "version": "1.0.0"}' > package.json
   ```

8. **Create VM (Zero Config)**
   ```bash
   vm create
   ```
   - **Record:**
     - Command started: [timestamp]
     - Command completed: [timestamp]
     - Total time: _____ minutes
     - Success? (Yes/No): _____
     - VM name created: _____
     - Did it auto-detect Node.js? (Yes/No): _____
     - Any errors: _____

9. **Verify VM is running**
   ```bash
   vm list
   vm status
   ```
   - **Record:**
     - VM appears in list? (Yes/No): _____
     - Status shows "running"? (Yes/No): _____
     - Resource usage visible? (Yes/No): _____

### Phase 5: Basic Usage (10-15 minutes)

**Objective:** Can the tester perform basic operations?

10. **SSH into VM**
    ```bash
    vm ssh
    ```
    - **Record:**
      - Connected successfully? (Yes/No): _____
      - Time to connect: _____ seconds
      - Working directory correct? (Yes/No): _____

11. **Test project detection inside VM**
    ```bash
    # Inside VM
    which node
    which npm
    node --version
    npm --version
    ls -la
    ```
    - **Record:**
      - Node.js installed? (Yes/No): _____
      - npm installed? (Yes/No): _____
      - Project files visible? (Yes/No): _____
      - Can create files that sync? (Yes/No): _____

12. **Exit and test other commands**
    ```bash
    # Exit SSH
    exit

    # Test other commands
    vm logs
    vm exec "echo 'Hello from VM'"
    vm stop
    vm start
    vm status
    ```
    - **Record:**
      - All commands work? (Yes/No): _____
      - Error messages helpful? (Yes/No): _____

### Phase 6: Cleanup (5 minutes)

**Objective:** Can the tester destroy the VM and clean up?

13. **Destroy VM**
    ```bash
    vm destroy
    ```
    - **Record:**
      - Destruction successful? (Yes/No): _____
      - Cleanup complete? (Yes/No): _____
      - Container fully removed (`docker ps -a`)? (Yes/No): _____

---

## Success Criteria

**Must Pass (Blockers):**
- [ ] ✅ Installation completes without errors
- [ ] ✅ First VM creation succeeds
- [ ] ✅ SSH connection works
- [ ] ✅ Project files are accessible in VM
- [ ] ✅ Auto-detection works for Node.js project
- [ ] ✅ VM can be destroyed cleanly

**Should Pass (UX Issues):**
- [ ] Total time < 60 minutes for complete onboarding
- [ ] No confusing error messages
- [ ] README clearly explains all steps
- [ ] Prerequisites are documented
- [ ] Help text is useful

**Nice to Have:**
- [ ] Time to first VM < 30 minutes
- [ ] Zero manual configuration needed
- [ ] Status commands provide clear feedback

---

## Data Collection

### Timing Metrics

| Phase | Expected Time | Actual Time | Status |
|-------|---------------|-------------|--------|
| Discovery | 5 min | _____ | ⬜ |
| Prerequisites | 20 min | _____ | ⬜ |
| Installation | 15 min | _____ | ⬜ |
| First VM | 20 min | _____ | ⬜ |
| Basic Usage | 15 min | _____ | ⬜ |
| Cleanup | 5 min | _____ | ⬜ |
| **TOTAL** | **60 min** | **_____** | ⬜ |

### Error Log

Record ALL errors encountered:

```
[Timestamp] [Phase] [Command]
Error message:
Resolution:
Time to resolve:
---
```

### UX Observations

**What went well:**
1.
2.
3.

**What was confusing:**
1.
2.
3.

**What was missing from documentation:**
1.
2.
3.

**Suggestions for improvement:**
1.
2.
3.

---

## Scoring Rubric

### Overall Experience (1-10)

- **10 = Perfect:** Everything just works, docs are clear, zero friction
- **7-9 = Good:** Minor issues, mostly smooth, documentation helpful
- **4-6 = Needs Work:** Several blockers, confusing docs, frustrating
- **1-3 = Poor:** Major issues, couldn't complete, bad experience

**Your Score:** _____ / 10

### Category Scores (1-5 each)

| Category | Score | Notes |
|----------|-------|-------|
| Documentation Clarity | _____ | |
| Installation Ease | _____ | |
| Error Messages | _____ | |
| First VM Experience | _____ | |
| Command Discoverability | _____ | |
| Overall Polish | _____ | |

---

## Deliverables

Please submit:

1. **Completed Test Report** (this document with all fields filled)
2. **Error Log** (all errors encountered with timestamps)
3. **Screen Recording** (optional but helpful)
4. **Terminal History** (`.bash_history` or command log)

### Submission Format

```markdown
## Test Report: Complete Beginner Onboarding

**Tester:** [Your Name]
**Date:** [YYYY-MM-DD]
**OS:** [Ubuntu 22.04 / macOS 13 / Windows 11 WSL2]
**Total Time:** [XX minutes]
**Overall Score:** [X/10]
**Status:** [✅ PASS / ❌ FAIL / ⚠️ PARTIAL]

### Summary
[2-3 sentence summary of experience]

### Blockers Encountered
[List any show-stopping issues]

### Top 3 Improvements Needed
1.
2.
3.

### Detailed Logs
[Attach completed test document]
```

---

## Follow-up Questions

After completing the test, please answer:

1. **Would you recommend this tool to a colleague?** (Yes/No/Maybe)
   - Why or why not?

2. **What was the most frustrating part?**

3. **What impressed you most?**

4. **If you could change one thing about the onboarding, what would it be?**

5. **How does this compare to other dev environment tools you've used?**

---

## Notes for Test Administrators

**Before Test:**
- Provide clean test environment
- Don't pre-install any dependencies
- Don't provide hints beyond the README

**During Test:**
- Observe but don't help unless tester is completely stuck
- Record all questions asked
- Note any visible frustration or confusion

**After Test:**
- Conduct brief interview about experience
- Gather detailed feedback on pain points
- Thank tester for their time!

**Expected Outcomes:**
- Identify gaps in documentation
- Find confusing error messages
- Discover installation pain points
- Validate "zero config" claims
- Measure true time-to-productivity

---

## Success Metrics

**This test passes if:**
- ✅ Tester completes all phases within 90 minutes
- ✅ No critical errors require external help
- ✅ Overall score ≥ 7/10
- ✅ Tester would recommend the tool

**This test is inconclusive if:**
- ⚠️ Tester completes but takes > 90 minutes
- ⚠️ Score is 5-6/10
- ⚠️ Multiple UX issues but no blockers

**This test fails if:**
- ❌ Cannot complete installation
- ❌ Cannot create first VM
- ❌ Score < 5/10
- ❌ Tester gives up before completion

---

## Related Tests

This is **Path 1** of 3 onboarding tests:

- ✅ **Proposal 15:** Complete Beginner (this test)
- 🔄 **Proposal 16:** Quick Start Developer (has Rust/Docker)
- 🔄 **Proposal 17:** CI/CD Automated (scripted setup)

**Compare results across all three paths to identify:**
- Which prerequisites cause the most friction
- Whether "zero config" works in practice
- If documentation serves all user levels
- Where automation can improve UX
