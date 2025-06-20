#!/usr/bin/env node
/**
 * VM Configuration Validator
 * Usage: node validate-config.js [path-to-vm.json]
 */

import fs from 'fs'
import path from 'path'
import { fileURLToPath } from 'url'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)

function findVmJson(startDir, maxLevels = 3) {
	let dir = startDir
	let level = 0
	
	while (level < maxLevels && dir !== path.dirname(dir)) {
		const vmJsonPath = path.join(dir, 'vm.json')
		if (fs.existsSync(vmJsonPath)) {
			return vmJsonPath
		}
		dir = path.dirname(dir)
		level++
	}
	
	return null
}

function deepMerge(base, override) {
	const result = { ...base }
	for (const [key, value] of Object.entries(override)) {
		if (
			typeof value === 'object' &&
			value !== null &&
			!Array.isArray(value) &&
			typeof result[key] === 'object' &&
			result[key] !== null &&
			!Array.isArray(result[key])
		) {
			result[key] = deepMerge(result[key], value)
		} else {
			result[key] = value
		}
	}
	return result
}

function validateConfig(configFile) {
	console.log(`Validating VM configuration: ${configFile}`)
	console.log('='.repeat(60))

	// Load default configuration
	const defaultsFile = path.join(__dirname, 'vm.json')
	if (!fs.existsSync(defaultsFile)) {
		console.log(`ERROR: Default configuration file not found: ${defaultsFile}`)
		return false
	}

	let defaultConfig, userConfig, projectConfig

	try {
		defaultConfig = JSON.parse(fs.readFileSync(defaultsFile, 'utf8'))
	} catch (e) {
		console.log(`ERROR: Invalid JSON in default config: ${e.message}`)
		return false
	}

	// Load project configuration if it exists
	if (fs.existsSync(configFile)) {
		try {
			userConfig = JSON.parse(fs.readFileSync(configFile, 'utf8'))
			projectConfig = deepMerge(defaultConfig, userConfig)
		} catch (e) {
			console.log(`ERROR: Invalid JSON in project config: ${e.message}`)
			return false
		}
	} else {
		console.log('WARNING: Project config file not found, using defaults only')
		projectConfig = defaultConfig
	}

	// Validation rules
	const errors = []
	const warnings = []

	// Provider validation
	const provider = projectConfig.provider
	if (provider && provider !== 'vagrant' && provider !== 'docker') {
		errors.push(`Invalid provider: "${provider}". Must be "vagrant" or "docker"`)
	}

	// Required project fields
	const projectName = projectConfig.project?.name
	if (!projectName || projectName.trim() === '') {
		errors.push('project.name is required')
	}

	const workspacePath = projectConfig.project?.workspace_path
	if (!workspacePath || workspacePath.trim() === '') {
		errors.push('project.workspace_path is required')
	}

	const hostname = projectConfig.project?.hostname
	if (!hostname || hostname.trim() === '') {
		errors.push('project.hostname is required')
	}

	// VM configuration validation
	if (!projectConfig.vm) {
		errors.push('vm configuration section is required')
	} else {
		// Provider-specific validation
		const currentProvider = projectConfig.provider || 'vagrant'
		
		if (currentProvider === 'vagrant') {
			const vmBox = projectConfig.vm.box
			if (!vmBox || vmBox.trim() === '') {
				errors.push('vm.box is required for Vagrant provider')
			}
		}

		const vmMemory = projectConfig.vm.memory
		if (!Number.isInteger(vmMemory) || vmMemory <= 0) {
			errors.push('vm.memory must be a positive integer')
		} else if (vmMemory < 1024) {
			warnings.push(`vm.memory is quite low (${vmMemory}MB), consider at least 1024MB`)
		}

		const vmCpus = projectConfig.vm.cpus
		if (!Number.isInteger(vmCpus) || vmCpus <= 0) {
			errors.push('vm.cpus must be a positive integer')
		}

		const vmUser = projectConfig.vm.user
		if (!vmUser || vmUser.trim() === '') {
			errors.push('vm.user is required')
		}
		
		// Port binding validation
		const portBinding = projectConfig.vm.port_binding
		if (portBinding && portBinding !== '127.0.0.1' && portBinding !== '0.0.0.0') {
			warnings.push(`Unusual port binding: "${portBinding}". Typically use "127.0.0.1" (localhost) or "0.0.0.0" (all interfaces)`)
		}
	}

	// Port validation
	if (projectConfig.ports) {
		Object.entries(projectConfig.ports).forEach(([service, port]) => {
			if (!Number.isInteger(port) || port <= 0 || port >= 65536) {
				errors.push(`ports.${service} must be a valid port number (1-65535)`)
			}
		})
	}

	// Terminal configuration validation
	if (projectConfig.terminal) {
		const emoji = projectConfig.terminal.emoji
		if (emoji && emoji.length > 10) {
			warnings.push('terminal.emoji is quite long, consider keeping it short')
		}

		const username = projectConfig.terminal.username
		if (username && username.length > 20) {
			warnings.push('terminal.username is quite long, consider keeping it under 20 characters')
		}
	}

	// Services validation (Docker-specific warnings)
	if (projectConfig.services) {
		const currentProvider = projectConfig.provider || 'vagrant'
		if (currentProvider === 'docker' && projectConfig.services.docker?.enabled) {
			warnings.push('Docker-in-Docker is enabled. This requires mounting the Docker socket and may have security implications.')
		}
	}

	// Report results
	console.log('\nValidation Results:')
	console.log('-'.repeat(20))
	console.log(`Provider: ${projectConfig.provider || 'vagrant'} (default)`)

	if (errors.length === 0 && warnings.length === 0) {
		console.log('✅ Configuration is valid!')
		return true
	}

	if (errors.length > 0) {
		console.log(`\n❌ ERRORS (${errors.length}):`)
		errors.forEach((error, index) => {
			console.log(`  ${index + 1}. ${error}`)
		})
	}

	if (warnings.length > 0) {
		console.log(`\n⚠️  WARNINGS (${warnings.length}):`)
		warnings.forEach((warning, index) => {
			console.log(`  ${index + 1}. ${warning}`)
		})
	}

	console.log(`\nConfiguration file: ${configFile}`)
	console.log(`Default config: ${defaultsFile}`)
	console.log('='.repeat(60))

	return errors.length === 0
}

// Main execution
const configFile = process.argv[2] || 
	process.env.VM_CONFIG || 
	findVmJson(process.cwd()) || 
	path.join(__dirname, '../../vm.json')

if (validateConfig(configFile)) {
	console.log('\n✅ VM configuration is ready to use!')
	process.exit(0)
} else {
	console.log('\n❌ VM configuration has errors that must be fixed before use.')
	process.exit(1)
}
