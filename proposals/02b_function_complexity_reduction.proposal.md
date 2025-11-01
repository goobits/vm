## Problem

Several functions exceed 100 lines with high cyclomatic complexity, making them difficult to understand, test, and maintain. Five critical functions range from 88-225 lines, with deep nesting and multiple responsibilities.

## Solution(s)

1. **Extract subfunctions:** Break down large functions into focused helper functions with clear responsibilities
2. **Use declarative patterns:** Replace imperative HashMap building with macros or code generation
3. **Apply guard clauses:** Reduce nesting depth by inverting conditionals and returning early
4. **Single responsibility:** Split functions handling multiple commands into separate command handlers

## Checklists

- [ ] **Refactor init.rs::execute() (225 lines):**
    - [ ] Extract service detection logic into `detect_services()`
    - [ ] Extract port allocation logic into `allocate_ports()`
    - [ ] Extract config building logic into `build_initial_config()`
    - [ ] Reduce main function to orchestration only
- [ ] **Refactor schema.rs::build_vm_schema_cache() (180 lines):**
    - [ ] Create declarative macro for schema entries
    - [ ] Group related schema entries into builder functions
    - [ ] Consider code generation for repetitive patterns
- [ ] **Refactor formatting.rs::flatten_config_to_shell() (104 lines):**
    - [ ] Extract recursive logic into separate helper
    - [ ] Extract shell escaping logic into utility function
    - [ ] Simplify main control flow
- [ ] **Refactor preset.rs::preset() (91 lines):**
    - [ ] Split into separate functions: `list_presets()`, `show_preset()`, `apply_preset()`
    - [ ] Move command dispatch logic to caller
    - [ ] Reduce main function complexity
- [ ] **Refactor schema.rs::build_global_schema_cache() (88 lines):**
    - [ ] Apply same patterns as vm_schema_cache refactor
    - [ ] Share common logic between global and vm schema builders
- [ ] **Reduce nesting depth:**
    - [ ] Audit functions in `vm-config/src/detector/mod.rs` for >4 nesting levels
    - [ ] Apply guard clauses to invert conditionals
    - [ ] Extract nested blocks into named functions
- [ ] **Verification:**
    - [ ] Verify no function exceeds 80 lines (except true orchestrators)
    - [ ] Run full test suite to ensure no regressions
    - [ ] Verify clippy complexity warnings resolved

## Success Criteria

- No functions exceed 100 lines
- Maximum nesting depth reduced to 3 levels
- Top 5 complex functions split into focused subfunctions
- All extracted functions have descriptive names and single responsibilities
- All tests pass without changes to public APIs

## Benefits

- Improves code readability and maintainability
- Reduces cognitive load when reviewing or modifying code
- Makes unit testing easier with focused functions
- Reduces cyclomatic complexity across codebase
- Makes onboarding new contributors faster
