#!/usr/bin/env node
// Docker provisioning script - generates docker-compose.yml from vm.json

const fs = require('fs');
const path = require('path');

// Template parser (simple mustache-like syntax)
function parseTemplate(template, data) {
    let result = template;
    
    // Replace simple variables
    Object.keys(data).forEach(key => {
        const regex = new RegExp(`{{${key}}}`, 'g');
        result = result.replace(regex, data[key]);
    });
    
    // Handle conditionals
    result = result.replace(/{{#(\w+)}}([\s\S]*?){{\/\1}}/g, (match, key, content) => {
        return data[key] ? content : '';
    });
    
    // Handle arrays/loops
    result = result.replace(/{{#(\w+)}}([\s\S]*?){{\/\1}}/g, (match, key, content) => {
        if (Array.isArray(data[key])) {
            return data[key].map(item => {
                let itemContent = content;
                Object.keys(item).forEach(itemKey => {
                    const regex = new RegExp(`{{${itemKey}}}`, 'g');
                    itemContent = itemContent.replace(regex, item[itemKey]);
                });
                return itemContent;
            }).join('');
        }
        return data[key] ? content : '';
    });
    
    return result;
}

// Generate docker-compose.yml from config
function generateDockerCompose(config, projectDir) {
    const templatePath = path.join(__dirname, 'docker-compose.yml.template');
    const template = fs.readFileSync(templatePath, 'utf8');
    
    // Prepare port mappings
    const ports = [];
    if (config.ports) {
        Object.entries(config.ports).forEach(([service, port]) => {
            ports.push({
                HOST_IP: config.vm.port_binding || '127.0.0.1',
                HOST_PORT: port,
                CONTAINER_PORT: port
            });
        });
    }
    
    // Prepare environment variables
    const environment = [];
    if (config.environment) {
        Object.entries(config.environment).forEach(([key, value]) => {
            environment.push({ KEY: key, VALUE: value });
        });
    }
    
    // Prepare template data
    const data = {
        PROJECT_NAME: config.project.name.replace(/[^a-zA-Z0-9]/g, ''),
        PROJECT_HOSTNAME: config.project.hostname,
        PROJECT_PATH: projectDir,
        VM_TOOL_PATH: path.join(__dirname, '../..'),
        WORKSPACE_PATH: config.project.workspace_path || '/workspace',
        PROJECT_USER: config.vm.user || 'vagrant',
        HOST_IP: config.vm.port_binding || '127.0.0.1',
        NODE_VERSION: config.versions.node || '22',
        NVM_VERSION: config.versions.nvm || 'v0.39.7',
        PNPM_VERSION: config.versions.pnpm || '9.15.4',
        TERMINAL_THEME: config.terminal.theme || 'dracula',
        TERMINAL_EMOJI: config.terminal.emoji || '🚀',
        TERMINAL_USERNAME: config.terminal.username || 'dev',
        SHOW_GIT_BRANCH: config.terminal?.show_git_branch !== false ? 'true' : 'false',
        SHOW_TIMESTAMP: config.terminal?.show_timestamp === true ? 'true' : 'false',
        DISPLAY: config.environment?.DISPLAY || ':99',
        NPM_PACKAGES: (config.npm_packages || []).join(' '),
        ENV_TEMPLATE_PATH: config.project?.env_template_path || '',
        BACKUP_PATTERN: config.project?.backup_pattern || '*backup*.sql.gz',
        PROJECT_ALIASES: Object.entries(config.aliases || {}).map(([k, v]) => `${k}=${v}`).join(';'),
        HEADLESS_BROWSER_ENABLED: config.services?.headless_browser?.enabled ? 'true' : 'false',
        PUPPETEER_EXECUTABLE_PATH: config.services?.headless_browser?.executable_path || '/usr/bin/chromium-browser',
        PORTS: ports,
        ENVIRONMENT: environment,
        
        // Services
        ENABLE_POSTGRESQL: config.services?.postgresql?.enabled,
        POSTGRES_USER: config.services?.postgresql?.user || 'postgres',
        POSTGRES_PASSWORD: config.services?.postgresql?.password || 'postgres',
        POSTGRES_DB: config.services?.postgresql?.database || 'myproject_dev',
        POSTGRES_PORT: config.ports?.postgresql || config.services?.postgresql?.port || 5432,
        
        ENABLE_REDIS: config.services?.redis?.enabled,
        REDIS_PORT: config.ports?.redis || config.services?.redis?.port || 6379,
        
        ENABLE_MONGODB: config.services?.mongodb?.enabled,
        MONGODB_DATABASE: config.project.name + '_dev',
        MONGODB_PORT: config.ports?.mongodb || config.services?.mongodb?.port || 27017
    };
    
    const dockerCompose = parseTemplate(template, data);
    const outputPath = path.join(projectDir, 'docker-compose.yml');
    
    fs.writeFileSync(outputPath, dockerCompose);
    console.log(`Generated docker-compose.yml at ${outputPath}`);
}

// Export for use in vm.sh
module.exports = { generateDockerCompose };

// Allow direct execution
if (require.main === module) {
    const configPath = process.argv[2];
    const projectDir = process.argv[3] || process.cwd();
    
    if (!configPath) {
        console.error('Usage: docker-provisioning.js <config-path> [project-dir]');
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