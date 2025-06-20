#!/bin/bash
# Restore PostgreSQL backup if available

set -e

BACKUP_PATTERN="${BACKUP_PATTERN:-*backup*.sql.gz}"
POSTGRES_HOST="${POSTGRES_HOST:-postgres}"
POSTGRES_PORT="${POSTGRES_PORT:-5432}"
POSTGRES_USER="${POSTGRES_USER:-postgres}"
POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-postgres}"
POSTGRES_DB="${POSTGRES_DB}"

# Wait for PostgreSQL to be ready
echo "Waiting for PostgreSQL to be ready..."
until PGPASSWORD="$POSTGRES_PASSWORD" psql -h "$POSTGRES_HOST" -p "$POSTGRES_PORT" -U "$POSTGRES_USER" -d "postgres" -c '\q' 2>/dev/null; do
    sleep 1
done

# Check for backups
if [ -d "/workspace/backups" ]; then
    BACKUP_FILE=$(find /workspace/backups -name "$BACKUP_PATTERN" -type f 2>/dev/null | sort -n | tail -1)
    
    if [ -n "$BACKUP_FILE" ] && [ -f "$BACKUP_FILE" ]; then
        echo "Found backup file: $BACKUP_FILE"
        echo "Restoring database from backup..."
        
        # Create database if it doesn't exist
        PGPASSWORD="$POSTGRES_PASSWORD" psql -h "$POSTGRES_HOST" -p "$POSTGRES_PORT" -U "$POSTGRES_USER" -d "postgres" -c "CREATE DATABASE $POSTGRES_DB" 2>/dev/null || true
        
        # Restore backup
        gunzip -c "$BACKUP_FILE" | PGPASSWORD="$POSTGRES_PASSWORD" psql -h "$POSTGRES_HOST" -p "$POSTGRES_PORT" -U "$POSTGRES_USER" -d "$POSTGRES_DB"
        
        echo "Database restored successfully!"
    else
        echo "No backup files found matching pattern: $BACKUP_PATTERN"
    fi
fi