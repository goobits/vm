#!/usr/bin/env node
/**
 * VM Configuration Manager
 * Primary purpose: Load, merge, validate, and output final configuration
 * Usage: node validate-config.js [--validate] [--get-config]
 */

import fs from 'fs'
import path from 'path'
import { fileURLToPath } from 'url'
import readline from 'readline'
import { execSync } from 'child_process'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)

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


function findVmJsonUpwards(startDir) {
	let currentDir = path.resolve(startDir)
	const rootDir = path.parse(currentDir).root
	
	while (currentDir !== rootDir) {
		const vmJsonPath = path.join(currentDir, 'vm.json')
		if (fs.existsSync(vmJsonPath)) {
			return vmJsonPath
		}
		currentDir = path.dirname(currentDir)
	}
	
	// Check root directory as well
	const rootVmJsonPath = path.join(rootDir, 'vm.json')
	if (fs.existsSync(rootVmJsonPath)) {
		return rootVmJsonPath
	}
	
	return null
}

function loadAndMergeConfig(customConfigPath = null) {
	// Config discovery: custom path, current directory vm.json, or default
	const localConfigPath = path.join(process.cwd(), 'vm.json')
	const defaultConfigPath = path.join(__dirname, 'vm.json')
	
	let configFileToLoad
	let configDirForScan = null
	if (customConfigPath === '__SCAN__') {
		// Scan up directory tree for vm.json
		const foundConfig = findVmJsonUpwards(process.cwd())
		if (foundConfig) {
			configFileToLoad = foundConfig
			// Store the directory where config was found for scan mode
			configDirForScan = path.dirname(foundConfig)
		} else {
			// Not found during scan, offer to create one
			console.error(`❌ No vm.json configuration file found scanning up from ${process.cwd()}`)
			console.error()
			console.error('The VM tool needs a vm.json file to configure your development environment.')
			console.error(`I can create a default vm.json file customized for the "${path.basename(process.cwd())}" project.`)
			console.error()
			console.error('Would you like me to create a vm.json file for this directory?')
			
			// Read user input synchronously - but only if we're in an interactive terminal
			let answer = 'n'
			if (process.stdin.isTTY && process.stdout.isTTY && process.stderr.isTTY) {
				try {
					answer = execSync('echo "Create vm.json? (y/n): " >&2 && read answer && echo $answer', { 
						encoding: 'utf8', 
						stdio: ['inherit', 'pipe', 'pipe'],
						timeout: 30000 // 30 second timeout
					}).trim()
				} catch (e) {
					console.error('Failed to read input, using default configuration...')
					answer = 'n'
				}
			} else {
				console.error('Non-interactive terminal detected, using default configuration...')
			}
			
			if (answer.toLowerCase() === 'y' || answer.toLowerCase() === 'yes') {
				// Create local vm.json based on default but with directory-specific name
				const defaultConfig = JSON.parse(fs.readFileSync(defaultConfigPath, 'utf8'))
				const dirName = path.basename(process.cwd())
				
				// Customize config for this directory
				const localConfig = {
					...defaultConfig,
					project: {
						...defaultConfig.project,
						name: dirName,
						hostname: dirName
					},
					terminal: {
						...defaultConfig.terminal,
						emoji: "🚀",
						username: dirName
					}
				}
				
				fs.writeFileSync(localConfigPath, JSON.stringify(localConfig, null, 2))
				console.error(`✅ Created vm.json for project: ${dirName}`)
				configFileToLoad = localConfigPath
			} else {
				console.error('Using default configuration...')
				configFileToLoad = defaultConfigPath
			}
		}
	} else if (customConfigPath) {
		// Handle custom config path (for tests and explicit --config usage)
		configFileToLoad = path.isAbsolute(customConfigPath) ? customConfigPath : path.join(process.cwd(), customConfigPath)
		if (!fs.existsSync(configFileToLoad)) {
			throw new Error(`Custom config file not found: ${configFileToLoad}`)
		}
		// When using custom config, we found it successfully, so use it
	} else {
		// Check if local vm.json exists
		if (fs.existsSync(localConfigPath)) {
			configFileToLoad = localConfigPath
		} else {
			// Prompt user to create local vm.json
			console.error(`❌ No vm.json configuration file found in ${process.cwd()}`)
			console.error()
			console.error('The VM tool needs a vm.json file to configure your development environment.')
			console.error(`I can create a default vm.json file customized for the "${path.basename(process.cwd())}" project.`)
			console.error()
			console.error('Would you like me to create a vm.json file for this directory?')
			
			// Read user input synchronously
			const answer = execSync('echo "Create vm.json? (y/n): " >&2 && read answer && echo $answer', { 
				encoding: 'utf8', 
				stdio: ['inherit', 'pipe', 'inherit'] 
			}).trim()
			
			if (answer.toLowerCase() === 'y' || answer.toLowerCase() === 'yes') {
				// Create local vm.json based on default but with directory-specific name
				const defaultConfig = JSON.parse(fs.readFileSync(defaultConfigPath, 'utf8'))
				const dirName = path.basename(process.cwd())
				
				// Customize config for this directory
				const localConfig = {
					...defaultConfig,
					project: {
						...defaultConfig.project,
						name: dirName,
						hostname: dirName
					},
					terminal: {
						...defaultConfig.terminal,
						emoji: "🚀",
						username: dirName
					}
				}
				
				fs.writeFileSync(localConfigPath, JSON.stringify(localConfig, null, 2))
				console.error(`✅ Created vm.json for project: ${dirName}`)
				configFileToLoad = localConfigPath
			} else {
				console.error('Using default configuration...')
				configFileToLoad = defaultConfigPath
			}
		}
	}
	
	// Load default configuration
	if (!fs.existsSync(defaultConfigPath)) {
		throw new Error(`Default configuration file not found: ${defaultConfigPath}`)
	}
	
	let defaultConfig, userConfig = {}, finalConfig
	
	try {
		defaultConfig = JSON.parse(fs.readFileSync(defaultConfigPath, 'utf8'))
	} catch (e) {
		throw new Error(`Invalid JSON in default config: ${e.message}`)
	}
	
	// Load user config if using a custom config or local config
	if (configFileToLoad !== defaultConfigPath) {
		try {
			userConfig = JSON.parse(fs.readFileSync(configFileToLoad, 'utf8'))
		} catch (e) {
			throw new Error(`Invalid JSON in project config: ${e.message}`)
		}
	}
	
	// Merge configurations (user config overrides defaults)
	finalConfig = deepMerge(defaultConfig, userConfig)
	
	return { finalConfig, configFileToLoad, isLocal: configFileToLoad === localConfigPath, configDirForScan }
}

function validateMergedConfig(config) {
	const errors = []
	const warnings = []

	// Provider validation
	const provider = config.provider
	if (provider && provider !== 'vagrant' && provider !== 'docker') {
		errors.push(`Invalid provider: "${provider}". Must be "vagrant" or "docker"`)
	}

	// Required project fields
	if (!config.project?.name?.trim()) {
		errors.push('project.name is required')
	}
	if (!config.project?.workspace_path?.trim()) {
		errors.push('project.workspace_path is required')
	}
	if (!config.project?.hostname?.trim()) {
		errors.push('project.hostname is required')
	}

	// VM configuration validation
	if (!config.vm) {
		errors.push('vm configuration section is required')
	} else {
		const currentProvider = config.provider || 'vagrant'
		
		if (currentProvider === 'vagrant') {
			if (!config.vm.box?.trim()) {
				errors.push('vm.box is required for Vagrant provider')
			}
		}

		if (!Number.isInteger(config.vm.memory) || config.vm.memory <= 0) {
			errors.push('vm.memory must be a positive integer')
		} else if (config.vm.memory < 1024) {
			warnings.push(`vm.memory is quite low (${config.vm.memory}MB), consider at least 1024MB`)
		}

		if (!Number.isInteger(config.vm.cpus) || config.vm.cpus <= 0) {
			errors.push('vm.cpus must be a positive integer')
		}

		if (!config.vm.user?.trim()) {
			errors.push('vm.user is required')
		}
		
		const portBinding = config.vm.port_binding
		if (portBinding && portBinding !== '127.0.0.1' && portBinding !== '0.0.0.0') {
			warnings.push(`Unusual port binding: "${portBinding}". Typically use "127.0.0.1" (localhost) or "0.0.0.0" (all interfaces)`)
		}
	}

	// Port validation
	if (config.ports) {
		Object.entries(config.ports).forEach(([service, port]) => {
			if (!Number.isInteger(port) || port <= 0 || port >= 65536) {
				errors.push(`ports.${service} must be a valid port number (1-65535)`)
			}
		})
	}

	// Terminal configuration validation
	if (config.terminal) {
		if (config.terminal.emoji?.length > 10) {
			warnings.push('terminal.emoji is quite long, consider keeping it short')
		}
		if (config.terminal.username?.length > 20) {
			warnings.push('terminal.username is quite long, consider keeping it under 20 characters')
		}
	}

	// Services validation (Docker-specific warnings)
	if (config.services) {
		const currentProvider = config.provider || 'vagrant'
		if (currentProvider === 'docker' && config.services.docker?.enabled) {
			warnings.push('Docker-in-Docker is enabled. This requires mounting the Docker socket and may have security implications.')
		}
	}

	return { errors, warnings }
}

// Main execution
const args = process.argv.slice(2)
const validateFlag = args.includes('--validate')
const getConfigFlag = args.includes('--get-config')
const customConfigPath = args.find(arg => !arg.startsWith('--'))

// Debug logging when VM_DEBUG is set
if (process.env.VM_DEBUG) {
	console.error('DEBUG: args =', args)
	console.error('DEBUG: customConfigPath =', customConfigPath)
	console.error('DEBUG: cwd =', process.cwd())
}

try {
	const { finalConfig, configFileToLoad, isLocal, configDirForScan } = loadAndMergeConfig(customConfigPath)
	const { errors, warnings } = validateMergedConfig(finalConfig)
	
	if (validateFlag) {
		// Verbose validation mode for debugging
		console.log(`Validating VM configuration: ${configFileToLoad}`)
		console.log('='.repeat(60))
		console.log(`Provider: ${finalConfig.provider || 'docker'} (default)`)
		
		if (errors.length === 0 && warnings.length === 0) {
			console.log('✅ Configuration is valid!')
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
		
		console.log(`\nConfiguration file: ${configFileToLoad}`)
		console.log('='.repeat(60))
		
		if (errors.length === 0) {
			console.log('\n✅ VM configuration is ready to use!')
			process.exit(0)
		} else {
			console.log('\n❌ VM configuration has errors that must be fixed before use.')
			process.exit(1)
		}
	} else {
		// Default mode: Output merged config for vm.sh or validate quietly
		if (errors.length > 0) {
			console.error(errors.join('; '))
			process.exit(1)
		}
		
		// Print the final, merged config object to stdout for the shell script to capture
		// Include the config directory if we're in scan mode
		if (configDirForScan) {
			finalConfig.__config_dir = configDirForScan
		}
		console.log(JSON.stringify(finalConfig))
		process.exit(0)
	}
} catch (e) {
	console.error(e.message)
	process.exit(1)
}
