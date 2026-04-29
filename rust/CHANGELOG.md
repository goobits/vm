# Changelog

## 5.0.0

- Rebuilt the public CLI around the humane v5 command surface: `run`, `ls`, `shell`, `exec`, `logs`, `copy`, `stop`, `rm`, `save`, `revert`, `package`, `config`, `tunnel`, `doctor`, `plugin`, and `system`.
- Moved lower-level system plumbing under `vm system`, including registry and base-image workflows.
- Kept database, fleet, and secret workflows as flat plugin-backed top-level commands: `vm db`, `vm fleet`, and `vm secret`.
- Updated docs to describe only the v5 command model.
- Preserved saved snapshots when removing active environments with `vm rm`.
