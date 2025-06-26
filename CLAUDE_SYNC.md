# Claude AI Data Sync

The VM tool now supports automatic synchronization of Claude AI data between your VMs and your host machine.

## Configuration

Add the following to your `vm.json`:

```json
{
  "claude_sync": true
}
```

## How it Works

When `claude_sync` is enabled:

- **Docker**: Creates a volume mount from `~/.claude/vms/{project_name}` to `/home/vagrant/.claude` in the container
- **Vagrant**: Creates a synced folder with the same paths

This allows you to:
- Monitor Claude token usage across all VMs
- Preserve session history when VMs are recreated
- Analyze Claude usage patterns between projects
- Keep all Claude data organized by project

## File Structure

With claude_sync enabled, your Claude data will be organized like this:

```
~/.claude/
├── projects/          # Your local Claude projects
└── vms/              # VM Claude data
    ├── myapp/        # Data from 'myapp' VM
    ├── api/          # Data from 'api' VM
    └── frontend/     # Data from 'frontend' VM
```

## Example

Enable Claude sync in your project:

```json
{
  "project": {
    "name": "myapp"
  },
  "claude_sync": true
}
```

This will sync Claude data to `~/.claude/vms/myapp/` on your host machine.

## Benefits

- **Centralized Monitoring**: Track Claude usage across all development environments
- **Data Persistence**: Never lose Claude interaction history
- **Cost Management**: Monitor API usage per project
- **Team Insights**: Share usage patterns (if using shared volumes)

## Notes

- Disabled by default to maintain backward compatibility
- Directories are created automatically on first use
- Each VM gets its own isolated subdirectory
- Works with both Docker and Vagrant providers