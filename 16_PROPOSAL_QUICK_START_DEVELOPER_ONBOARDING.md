# Proposal: Quick Start Developer Onboarding Test

**Status:** 🧪 Onboarding Test - Path 2
**Priority:** P0
**Complexity:** Low (Testing Only)
**Estimated Effort:** 15-30 minutes
**Test Type:** Fast Path - Prerequisites Met

---

## Purpose

This proposal tests the **experienced developer onboarding experience** for developers who have:
- ✅ Rust toolchain already installed
- ✅ Docker already installed and running
- ✅ Comfortable with CLI tools
- ✅ Want to get productive quickly
- 🎯 **Goal: Zero to productive in under 15 minutes**

**Goal:** Measure the "quick start" experience and validate that experienced developers can onboard in minimal time without reading extensive documentation.

---

## Test Persona

**Name:** Jordan (Senior Backend Engineer)
**Background:**
- 5+ years software development experience
- Already uses Docker daily
- Has Rust installed for other projects
- Wants to quickly test VM tool for team adoption
- Values speed and efficiency

**Pain Points to Test:**
- Can I get started in < 15 minutes?
- Do I need to read extensive docs?
- Does the tool respect my existing setup?
- Can I use advanced features immediately?

---

## Test Environment

**Starting State:**
```bash
# Prerequisites MUST be installed
cargo --version     # Should return: cargo 1.x.x
docker --version    # Should return: Docker version 20.x+
docker ps           # Should succeed (daemon running)
which vm            # Should return: not found
```

**Test Machine Requirements:**
- Ubuntu 22.04 / macOS 13+ / Windows 10+ with WSL2
- Rust toolchain 1.70+
- Docker 20.10+
- Internet connection
- At least 2GB free disk space

**If prerequisites missing, this test is invalid** - use Proposal 15 instead.

---

## Test Procedure

### Phase 1: Lightning Install (2-3 minutes)

**Objective:** Can an experienced dev install in under 3 minutes?

1. **Navigate to project**
   - [ ] Go to project repository
   - [ ] Scan README for installation command (don't read everything)

2. **Install from Cargo**
   ```bash
   time cargo install vm
   ```
   - **Record:**
     - Start time: [timestamp]
     - End time: [timestamp]
     - Total seconds: _____ (Target: < 180s)
     - Compilation successful: (Yes/No): _____

3. **Quick verification**
   ```bash
   vm --version
   vm --help | head -20
   ```
   - **Record:**
     - Version: _____
     - Help output clear enough to proceed: (Yes/No): _____

**Phase 1 Score:** ⬜ PASS (< 3 min) | ⬜ ACCEPTABLE (3-5 min) | ⬜ FAIL (> 5 min)

### Phase 2: Real-World Quick Start (5-10 minutes)

**Objective:** Can you use the tool with a real project immediately?

4. **Choose your own project type** (pick ONE that matches your experience)

   **Option A: Node.js/React Project**
   ```bash
   git clone https://github.com/vercel/next.js
   cd next.js/examples/blog-starter
   vm create
   ```

   **Option B: Python/Django Project**
   ```bash
   mkdir ~/test-django && cd ~/test-django
   pip freeze > requirements.txt  # Use your current env
   echo "Django>=4.2" > requirements.txt
   vm create
   ```

   **Option C: Rust Project**
   ```bash
   cargo new test-rust-app
   cd test-rust-app
   vm create
   ```

   **Option D: Existing Work Project** (if allowed)
   ```bash
   cd ~/work/your-actual-project
   vm create
   ```

5. **Track the experience**
   - **Record:**
     - Project type chosen: _____
     - Command executed: _____
     - Start time: [timestamp]
     - Did it auto-detect framework? (Yes/No): _____
     - Time to VM ready: _____ seconds (Target: < 120s)
     - Success: (Yes/No): _____
     - Errors: _____

6. **Test immediate productivity**
   ```bash
   vm ssh
   # Inside VM - verify your project works
   # For Node: npm install && npm run dev
   # For Python: pip install -r requirements.txt
   # For Rust: cargo build
   ```
   - **Record:**
     - Project files accessible: (Yes/No): _____
     - Can run project commands: (Yes/No): _____
     - Dependencies installed correctly: (Yes/No): _____

**Phase 2 Score:** ⬜ PASS (< 10 min) | ⬜ ACCEPTABLE (10-15 min) | ⬜ FAIL (> 15 min)

### Phase 3: Advanced Features (5-10 minutes)

**Objective:** Can you discover and use advanced features without reading docs?

7. **Multi-instance workflow** (test without reading docs first)
   ```bash
   vm create --instance dev
   vm create --instance staging
   vm list
   vm ssh dev
   exit
   vm ssh staging
   exit
   ```
   - **Record:**
     - Figured out multi-instance syntax: (Yes/No): _____
     - Both instances created: (Yes/No): _____
     - Can switch between instances: (Yes/No): _____
     - Time to figure out: _____ minutes

8. **Test temp VM feature**
   ```bash
   cd ~/test-project
   vm temp create ./src ./tests
   vm temp ssh
   # Verify mounts
   ls
   exit
   vm temp destroy
   ```
   - **Record:**
     - Discovered temp feature: (How: _____):
     - Feature worked as expected: (Yes/No): _____
     - Use case clear: (Yes/No): _____

9. **Test configuration**
   ```bash
   vm init
   cat vm.yaml
   # Edit ports or resources
   vim vm.yaml  # or code vm.yaml
   vm validate
   ```
   - **Record:**
     - Config file intuitive: (Yes/No): _____
     - Validation helpful: (Yes/No): _____
     - Could customize without docs: (Yes/No): _____

**Phase 3 Score:** ⬜ PASS (all features discoverable) | ⬜ PARTIAL | ⬜ FAIL (needed docs)

### Phase 4: Cleanup & Edge Cases (2-5 minutes)

**Objective:** Test the full lifecycle and edge cases

10. **Test destroy operations**
    ```bash
    vm destroy dev --force
    vm destroy staging
    vm destroy  # Should prompt or fail with helpful message
    vm list     # Should show empty or remaining VMs
    ```
    - **Record:**
      - Destroy commands clear: (Yes/No): _____
      - Safety prompts appropriate: (Yes/No): _____
      - Cleanup successful: (Yes/No): _____

11. **Test error handling** (intentional failures)
    ```bash
    vm ssh nonexistent-vm    # Should fail gracefully
    vm destroy --all         # Should work or prompt
    docker ps -a | grep vm   # Should show clean state
    ```
    - **Record:**
      - Error messages helpful: (Yes/No): _____
      - Suggested fixes provided: (Yes/No): _____
      - Clean error recovery: (Yes/No): _____

**Phase 4 Score:** ⬜ PASS (clean lifecycle) | ⬜ ACCEPTABLE | ⬜ FAIL

---

## Success Criteria

### Hard Requirements (Must Pass)
- [ ] ✅ Installation completes in < 5 minutes
- [ ] ✅ First VM creation in < 3 minutes
- [ ] ✅ SSH works immediately
- [ ] ✅ Auto-detection works for chosen framework
- [ ] ✅ Total time to productivity < 15 minutes

### Soft Requirements (Should Pass)
- [ ] No documentation needed for basic usage
- [ ] Advanced features discoverable via `--help`
- [ ] Error messages provide clear next steps
- [ ] Configuration is intuitive
- [ ] Cleanup is straightforward

### Nice to Have
- [ ] Installation < 2 minutes
- [ ] Multi-instance workflow obvious
- [ ] Temp VMs discoverable without docs
- [ ] Zero friction from start to finish

---

## Data Collection

### Speed Test Results

| Metric | Target | Actual | Status |
|--------|--------|--------|--------|
| Installation Time | < 3 min | _____ | ⬜ |
| First VM Creation | < 2 min | _____ | ⬜ |
| Time to SSH | < 30 sec | _____ | ⬜ |
| Total Onboarding | < 15 min | _____ | ⬜ |
| Advanced Features | < 10 min | _____ | ⬜ |

### Friction Log

For EACH moment of confusion or delay, record:

```
[HH:MM] - Stuck on: [what you were trying to do]
Attempted: [what you tried]
Resolution: [how you solved it]
Time lost: [seconds]
Should have been: [what would have been clearer]
```

### Feature Discovery

How did you discover each feature?

| Feature | Discovery Method | Time to Find |
|---------|------------------|--------------|
| Basic create | ⬜ README ⬜ `--help` ⬜ Guessed | _____ |
| Multi-instance | ⬜ README ⬜ `--help` ⬜ Guessed | _____ |
| Temp VMs | ⬜ README ⬜ `--help` ⬜ Guessed | _____ |
| Config file | ⬜ README ⬜ `--help` ⬜ Guessed | _____ |
| Port mapping | ⬜ README ⬜ `--help` ⬜ Guessed | _____ |

---

## Developer Experience Survey

### Rate 1-5 (5 = Excellent)

| Aspect | Score | Comments |
|--------|-------|----------|
| Installation Speed | _____ | |
| Time to First Success | _____ | |
| Command Discoverability | _____ | |
| Error Message Quality | _____ | |
| Auto-Detection Accuracy | _____ | |
| Advanced Feature UX | _____ | |
| Overall Developer Experience | _____ | |

### Quick Questions

1. **Did you need to read documentation?** (Yes/No/Partially)
   - Which parts: _____

2. **Compared to similar tools (Docker Compose, Vagrant, etc.), this was:**
   - ⬜ Much faster
   - ⬜ Somewhat faster
   - ⬜ About the same
   - ⬜ Slower

3. **Would you adopt this for your team?** (Yes/No/Maybe)
   - Why: _____

4. **Biggest time-saver:**

5. **Biggest friction point:**

6. **Most surprising feature:**

7. **Missing feature you expected:**

---

## Developer Scoring

### Overall Score: _____ / 10

**10 = World-class:** Fastest onboarding ever, zero friction, delightful
**8-9 = Excellent:** Very smooth, minimal docs needed, would recommend
**6-7 = Good:** Works well, some rough edges, would use
**4-5 = Acceptable:** Gets job done, needs polish, might use
**1-3 = Poor:** Too slow, too confusing, wouldn't use

### Recommendation

Would you recommend this tool to your team?

- ⬜ **Strong Yes** - Already planning to share
- ⬜ **Yes** - Would recommend with caveats
- ⬜ **Maybe** - Needs work before recommending
- ⬜ **No** - Not ready for team adoption

**Reason:**

---

## Competitive Comparison

If you've used similar tools, compare:

| Tool | Setup Time | Ease of Use | Auto-Config | Overall |
|------|------------|-------------|-------------|---------|
| Docker Compose | _____ | _____ | _____ | _____ |
| Vagrant | _____ | _____ | _____ | _____ |
| DevContainer | _____ | _____ | _____ | _____ |
| **VM Tool** | _____ | _____ | _____ | _____ |

**This tool's killer feature:**

**This tool's biggest gap:**

---

## Deliverables

Submit:

1. **Completed test report** (this document)
2. **Friction log** (every moment of delay)
3. **Terminal history** (command log)
4. **Timing breakdown** (for each phase)

### Report Format

```markdown
## Quick Start Developer Test

**Tester:** [Name]
**Date:** [YYYY-MM-DD]
**OS:** [System]
**Experience:** [Years in development]
**Total Time:** [MM:SS]
**Score:** [X/10]
**Recommendation:** [Strong Yes / Yes / Maybe / No]

### TL;DR
[One paragraph: Would you use this? Why/why not?]

### Speed Results
- Installation: [MM:SS] (Target: 03:00)
- First VM: [MM:SS] (Target: 02:00)
- Total: [MM:SS] (Target: 15:00)

### Top 3 Wins
1.
2.
3.

### Top 3 Friction Points
1.
2.
3.

### One-Sentence Verdict
[How would you describe this tool to a colleague?]
```

---

## Test Scenarios by Stack

### Frontend Developers
- Test with: Next.js, React, Vue, Angular
- Focus on: Port forwarding, hot reload, npm integration

### Backend Developers
- Test with: Django, Flask, Rails, Express
- Focus on: Database services, API ports, environment variables

### DevOps Engineers
- Test with: Multi-service apps, Docker Compose projects
- Focus on: Multi-instance, configuration, resource limits

### Full-Stack Developers
- Test with: Monorepo or full-stack app
- Focus on: Multi-framework detection, service orchestration

**Choose the scenario matching your expertise for most authentic test results.**

---

## Success Metrics

**This test PASSES if:**
- ✅ Total time < 15 minutes
- ✅ Zero documentation required for basics
- ✅ Score ≥ 8/10
- ✅ Tester would recommend to team

**This test is ACCEPTABLE if:**
- ⚠️ Total time 15-20 minutes
- ⚠️ Minimal documentation needed
- ⚠️ Score 6-7/10
- ⚠️ Tester would consider using

**This test FAILS if:**
- ❌ Total time > 20 minutes
- ❌ Extensive documentation required
- ❌ Score < 6/10
- ❌ Tester would not recommend

---

## Notes for Fast-Track Testing

**Key Differences from Beginner Test:**
- Focus on speed, not exploration
- Assume CLI competence
- Test advanced features immediately
- Compare to existing tools in your workflow

**Time Limits:**
- Stop if installation takes > 5 minutes (FAIL)
- Stop if first VM takes > 5 minutes (FAIL)
- Stop if total exceeds 30 minutes (FAIL)

**This is a SPEED TEST** - work as fast as you normally would, don't overthink.

---

## Related Tests

This is **Path 2** of 3 onboarding tests:

- 🔄 **Proposal 15:** Complete Beginner (no prerequisites)
- ✅ **Proposal 16:** Quick Start Developer (this test)
- 🔄 **Proposal 17:** CI/CD Automated (scripted setup)

**Compare all three to identify:**
- Which path is fastest (should be this one)
- Where experienced devs still hit friction
- Whether auto-detection works across skill levels
- If advanced features are discoverable
