-- Add fields for provider integration
ALTER TABLE workspaces ADD COLUMN provider_id TEXT; -- Docker container ID, Tart VM name, etc.
ALTER TABLE workspaces ADD COLUMN connection_info TEXT; -- JSON: { ip, ports, ssh_command }
ALTER TABLE workspaces ADD COLUMN error_message TEXT; -- Error if provision failed
