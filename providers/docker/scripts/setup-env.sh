#!/bin/bash
# Setup environment from template

set -e

ENV_TEMPLATE_PATH="${ENV_TEMPLATE_PATH}"
POSTGRES_HOST="${POSTGRES_HOST:-postgres}"
POSTGRES_PORT="${POSTGRES_PORT:-5432}"
POSTGRES_USER="${POSTGRES_USER:-postgres}"
POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-postgres}"
POSTGRES_DB="${POSTGRES_DB}"
REDIS_HOST="${REDIS_HOST:-redis}"
REDIS_PORT="${REDIS_PORT:-6379}"

if [ -n "$ENV_TEMPLATE_PATH" ] && [ -f "/workspace/$ENV_TEMPLATE_PATH" ]; then
    echo "Setting up environment from template: $ENV_TEMPLATE_PATH"
    
    # Get directory of template
    ENV_DIR=$(dirname "/workspace/$ENV_TEMPLATE_PATH")
    
    # Copy template to .env
    cp "/workspace/$ENV_TEMPLATE_PATH" "$ENV_DIR/.env"
    
    # Configure DATABASE_URL
    if grep -q "^DATABASE_URL=" "$ENV_DIR/.env"; then
        sed -i "s|^DATABASE_URL=.*|DATABASE_URL=postgresql://${POSTGRES_USER}:${POSTGRES_PASSWORD}@${POSTGRES_HOST}:${POSTGRES_PORT}/${POSTGRES_DB}|" "$ENV_DIR/.env"
    else
        echo "DATABASE_URL=postgresql://${POSTGRES_USER}:${POSTGRES_PASSWORD}@${POSTGRES_HOST}:${POSTGRES_PORT}/${POSTGRES_DB}" >> "$ENV_DIR/.env"
    fi
    
    # Configure REDIS_URL
    if grep -q "^REDIS_URL=" "$ENV_DIR/.env"; then
        sed -i "s|^REDIS_URL=.*|REDIS_URL=redis://${REDIS_HOST}:${REDIS_PORT}|" "$ENV_DIR/.env"
    else
        echo "REDIS_URL=redis://${REDIS_HOST}:${REDIS_PORT}" >> "$ENV_DIR/.env"
    fi
    
    echo "Environment file created at: $ENV_DIR/.env"
fi