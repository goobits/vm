**Analyze project files for cleanup. Generate a tree view with action recommendations:**

* **DELETE**: Unused artifacts, old dev files, stale logs outside /log directory
* **RENAME**: Files that would be better off named something more appropriate
* **KEEP**: Active code, configs, essential assets, /log directory contents, /data directory contents
* **MOVE**: Misplaced files that belong elsewhere in the project structure

**Exclude from deletion**: /log/*, /data/*, active dependencies, .egg-info, .ruff_cache (stuff like that), current configs including vm.yml, .env, venv

**Focus on**: Temporary files, old backups, unused scripts, redundant assets, developer artifacts, build remnants

Output format: File tree with action comment for each item.

$ARGUMENTS