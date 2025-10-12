 Current State

  Data DOES persist across VM destroy:
  - PostgreSQL data: ~/.vm/data/postgres (from global_config.rs:358)
  - Redis data: ~/.vm/data/redis
  - MongoDB data: ~/.vm/data/mongodb

  When you vm destroy:
  1. ✅ Removes the VM container
  2. ✅ Decrements service reference count
  3. ✅ Stops PostgreSQL container when last VM removed
  4. ✅ Data directory stays intact - no deletion
  5. ✅ Next vm create reattaches to same data

  Each project gets:
  - Its own database NAME (e.g., goobits-prompt-palette_dev)
  - Shared PostgreSQL server instance
  - Isolated data within that server

  Problems I See

  ❌ No Backup Mechanism

  - No vm db backup command
  - No automatic backups before risky operations
  - No snapshots or versioning
  - No disaster recovery path

  ❌ No Clear Documentation

  - User doesn't know data persists
  - Could accidentally delete ~/.vm/data/ thinking it's cache
  - No warning on first destroy

  ❌ No Export/Import

  - Can't easily move databases between machines
  - Can't share database snapshots with team
  - No CI/CD seeding strategy

  ❌ Version Upgrade Risk

  - If global PostgreSQL version changes (15 → 16)
  - Old data might be incompatible
  - No migration path

  Recommended Solution

  P1: Add Database Management Commands

  vm db backup [name]           # pg_dump to ~/.vm/backups/postgres/
  vm db restore [name]          # pg_restore from backup
  vm db list                    # Show all databases + backups
  vm db export [name] [file]    # Export to SQL file
  vm db import [file]           # Import SQL file
  vm db size                    # Show disk usage per database
  vm db reset [name] --force    # Drop and recreate database

  P2: Improve Destroy UX

  First destroy should warn:
  ⚠️  Destroying VM 'goobits-prompt-palette'

  📊 Database: Your PostgreSQL data will persist
     Location: ~/.vm/data/postgres
     Database: goobits_prompt_palette_dev (42 MB)

  💡 Tip: Create a backup first
     vm db backup before-destroy

  Continue? (y/N)

  P3: Auto-Backup Option

  In global config:
  services:
    postgresql:
      enabled: true
      auto_backup: true
      backup_retention: 7  # Keep last 7 backups

  Automatically creates timestamped backup on:
  - vm destroy (if last VM using DB)
  - Before PostgreSQL version upgrade
  - Manual: vm db backup

  P4: Per-Project Backup Strategy

  In vm.yaml:
  services:
    postgresql:
      database: '{{ project.name }}_dev'
      backup_on_destroy: true
      seed_file: './database/seed.sql'  # Auto-import on first create

  Implementation Plan

  Files to create:
  rust/vm/src/commands/db.rs        # New db subcommand
  rust/vm/src/db_backup.rs          # Backup/restore logic

  Files to modify:
  rust/vm/src/commands/vm_ops/destroy.rs  # Add warning
  rust/vm/src/service_manager.rs          # Backup hooks
  rust/vm/src/cli.rs                      # Add db subcommand

  Key functions:
  // db_backup.rs
  pub async fn create_backup(db_name: &str, backup_name: Option<&str>) -> Result<PathBuf>
  pub async fn restore_backup(backup_name: &str) -> Result<()>
  pub async fn list_backups() -> Result<Vec<BackupInfo>>
  pub async fn export_to_sql(db_name: &str, file: &Path) -> Result<()>
  pub async fn import_from_sql(file: &Path) -> Result<()>

  Quick Win

  We could add a simple warning to destroy right now with minimal code:

  // In destroy.rs
  if !state_file.exists() {
      vm_println!("\n💡 Tip: Your database data persists after destroy");
      vm_println!("   Location: ~/.vm/data/postgres");
      vm_println!("   To delete data: rm -rf ~/.vm/data/postgres\n");
  }

  Want me to implement the backup commands? It's a high-value feature that would make the tool much safer for production use.
