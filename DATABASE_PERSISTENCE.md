# Database Persistence

Enable persistent database storage that survives VM rebuilds by storing data in your project's `.vm/data/` directory.

## Configuration

```json
{
  "persist_databases": true
}
```

## How It Works

When enabled, database data directories are mounted from your project:

```
myproject/
├── src/
├── vm.json
└── .vm/
    └── data/
        ├── postgres/   # PostgreSQL data files
        ├── redis/      # Redis persistence files
        ├── mongodb/    # MongoDB data files
        └── mysql/      # MySQL data files
```

## Benefits

- **Survives provisioning**: Data persists through `vm provision`
- **Portable**: Include in project backups or move between machines
- **Inspectable**: Browse database files directly (when stopped)
- **Universal**: Same approach for Docker and Vagrant

## Important Notes

### Performance
- Native performance on Linux
- Slight overhead on macOS/Windows due to file system virtualization
- For best performance, keep `persist_databases: false` in development

### Storage
- PostgreSQL: Can grow large (GBs) depending on usage
- Redis: Usually small unless using heavy persistence
- MongoDB: Variable, can be very large
- MySQL: Similar to PostgreSQL, can grow large

### Git
Add `.vm/` to your `.gitignore`:
```gitignore
.vm/
```

## Usage Example

1. Enable in your vm.json:
   ```json
   {
     "services": {
       "postgresql": { "enabled": true }
     },
     "persist_databases": true
   }
   ```

2. Start your VM:
   ```bash
   vm up
   ```

3. Your database data now persists in `.vm/data/postgres/`

## Backup & Restore

### Backup (Safe Method)
```bash
# While VM is running
vm exec pg_dump -U postgres dbname > backup.sql
```

### Backup (File Copy)
```bash
# Must stop VM first
vm halt
cp -r .vm/data/postgres .vm/data/postgres.backup
vm up
```

### Clean Data
```bash
vm halt
rm -rf .vm/data/postgres
vm up  # Starts with fresh database
```

## When to Use

✅ **Enable when:**
- You have important development data
- You frequently rebuild your VM
- You want to share database state

❌ **Keep disabled when:**
- You want maximum performance
- You prefer clean database state
- You're just testing

## Technical Details

- **Docker**: Bind mounts with `:delegated` flag for performance
- **Vagrant**: Synced folders with proper ownership
- **Real-time**: Changes write directly to host file system
- **Binary format**: Don't edit database files while running!