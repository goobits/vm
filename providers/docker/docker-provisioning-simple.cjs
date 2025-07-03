#!/usr/bin/env node
// Simplified Docker provisioning script

const fs = require('fs');
const path = require('path');
const os = require('os');
const { execSync } = require('child_process');

function generateDockerCompose(config, projectDir) {
    const template = `services:
  {{PROJECT_NAME}}:
    build:
      context: {{VM_TOOL_PATH}}
      dockerfile: providers/docker/Dockerfile
      args:
        PROJECT_USER: "{{PROJECT_USER}}"
        PROJECT_UID: "{{PROJECT_UID}}"
        PROJECT_GID: "{{PROJECT_GID}}"
    container_name: {{PROJECT_NAME}}-dev
    hostname: {{PROJECT_HOSTNAME}}
    tty: true
    stdin_open: true
    environment:
      - LANG=en_US.UTF-8
      - LC_ALL=en_US.UTF-8
      - TZ={{TIMEZONE}}
    volumes:
      - {{PROJECT_PATH}}:{{WORKSPACE_PATH}}:delegated
      - {{VM_TOOL_PATH}}:/vm-tool:ro
      - /var/run/docker.sock:/var/run/docker.sock:ro
      - {{PROJECT_NAME}}_nvm:/home/{{PROJECT_USER}}/.nvm
      - {{PROJECT_NAME}}_cache:/home/{{PROJECT_USER}}/.cache
      - {{PROJECT_NAME}}_config:/tmp{{CLAUDE_SYNC_VOLUME}}{{GEMINI_SYNC_VOLUME}}{{DATABASE_VOLUMES}}{{PORTS_SECTION}}
    networks:
      - {{PROJECT_NAME}}_network
    cap_add:
      - SYS_PTRACE
    security_opt:
      - seccomp:unconfined

networks:
  {{PROJECT_NAME}}_network:
    driver: bridge

volumes:
  {{PROJECT_NAME}}_nvm:
  {{PROJECT_NAME}}_cache:
  {{PROJECT_NAME}}_config:`;

    // Get host user/group IDs for proper file permissions
    const hostUid = execSync('id -u', { encoding: 'utf8' }).trim();
    const hostGid = execSync('id -g', { encoding: 'utf8' }).trim();

    // Prepare data
    const data = {
        PROJECT_NAME: config.project.name.replace(/[^a-zA-Z0-9]/g, ''),
        PROJECT_HOSTNAME: config.project.hostname,
        PROJECT_PATH: projectDir,
        VM_TOOL_PATH: path.join(__dirname, '../..'),
        WORKSPACE_PATH: config.project.workspace_path || '/workspace',
        PROJECT_USER: config.vm?.user || 'vagrant',
        PROJECT_UID: hostUid,
        PROJECT_GID: hostGid,
        TIMEZONE: config.vm?.timezone || 'UTC'
    };

    // Handle ports section
    let portsSection = '';
    if (config.ports && Object.keys(config.ports).length > 0) {
        const portLines = Object.entries(config.ports).map(([service, port]) => {
            const hostIp = config.vm?.port_binding || '127.0.0.1';
            return `      - "${hostIp}:${port}:${port}"`;
        });
        portsSection = '\n    ports:\n' + portLines.join('\n');
    }
    data.PORTS_SECTION = portsSection;

    // Handle Claude sync volume
    let claudeSyncVolume = '';
    if (config.claude_sync === true) {
        const hostPath = path.join(os.homedir(), '.claude', 'vms', data.PROJECT_NAME);
        const containerPath = `/home/${data.PROJECT_USER}/.claude`;
        claudeSyncVolume = `\n      - ${hostPath}:${containerPath}:delegated`;
    }
    data.CLAUDE_SYNC_VOLUME = claudeSyncVolume;

    // Handle Gemini sync volume
    let geminiSyncVolume = '';
    if (config.gemini_sync === true) {
        const hostPath = path.join(os.homedir(), '.gemini', 'vms', data.PROJECT_NAME);
        const containerPath = `/home/${data.PROJECT_USER}/.gemini`;
        geminiSyncVolume = `\n      - ${hostPath}:${containerPath}:delegated`;
    }
    data.GEMINI_SYNC_VOLUME = geminiSyncVolume;

    // Handle database persistence volumes
    let databaseVolumes = '';
    if (config.persist_databases === true) {
        const vmDataPath = path.join(projectDir, '.vm', 'data');
        
        // PostgreSQL
        if (config.services?.postgresql?.enabled) {
            databaseVolumes += `\n      - ${vmDataPath}/postgres:/var/lib/postgresql:delegated`;
        }
        
        // Redis
        if (config.services?.redis?.enabled) {
            databaseVolumes += `\n      - ${vmDataPath}/redis:/var/lib/redis:delegated`;
        }
        
        // MongoDB
        if (config.services?.mongodb?.enabled) {
            databaseVolumes += `\n      - ${vmDataPath}/mongodb:/var/lib/mongodb:delegated`;
        }
        
        // MySQL
        if (config.services?.mysql?.enabled) {
            databaseVolumes += `\n      - ${vmDataPath}/mysql:/var/lib/mysql:delegated`;
        }
        
    }
    // Always set DATABASE_VOLUMES (empty string if no databases to persist)
    data.DATABASE_VOLUMES = databaseVolumes;

    // Simple replacement
    let result = template;
    Object.entries(data).forEach(([key, value]) => {
        const regex = new RegExp(`{{${key}}}`, 'g');
        result = result.replace(regex, value);
    });

    const outputPath = path.join(projectDir, 'docker-compose.yml');
    fs.writeFileSync(outputPath, result);
    console.log(`Generated docker-compose.yml at ${outputPath}`);
}

// Allow direct execution
if (require.main === module) {
    const configPath = process.argv[2];
    const projectDir = process.argv[3] || process.cwd();
    
    if (!configPath) {
        console.error('Usage: docker-provisioning-simple.cjs <config-path> [project-dir]');
        process.exit(1);
    }
    
    try {
        const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
        generateDockerCompose(config, projectDir);
    } catch (error) {
        console.error('Error:', error.message);
        process.exit(1);
    }
}

module.exports = { generateDockerCompose };