# Dependency PR and security closure tracker

Active scope: resolve all 18 PRs open after CLI acceptance closeout, enable and
inspect repository security alerts, fix actionable findings, validate the combined
changes, and clean only task-owned branches/resources. Package image maintenance
and its acceptance stay within this tracker. Existing running VMs are preserved.

- [x] Inventory PRs #53, #66, #67, #68, #71, #72, #73, #75, #76, #77, #78,
  #79, #80, #82, #83, #84, #85, #86.
- [x] Enable Dependabot alerts/security updates and secret scanning/push protection.
- [ ] Integrate compatible Cargo and immutable GitHub Action updates.
- [ ] Refresh supported container bases; validate Rust builder/runtime compatibility.
- [ ] Record disposition of Ubuntu26 and Node26 proposals against supported-runtime policy.
- [ ] Add CodeQL analysis for Rust and GitHub Actions; inspect all security alerts.
- [ ] Run dependency security policy, workspace tests, Clippy, formatting, CLI docs,
  workflow checks, and live package acceptance; fix every actionable finding.
- [ ] Merge validated changes, close all superseded/declined PRs with evidence,
  remove their branches, and verify no unresolved PRs/security alerts remain.
- [ ] Move completed evidence to maintained development documentation and clean
  temporary files, images, and the task branch.
