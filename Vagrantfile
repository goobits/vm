# -*- mode: ruby -*-
# vi: set ft=ruby :

require 'json'

# Deep merge helper function
def deep_merge(base, override)
  base.merge(override) do |key, base_val, override_val|
    if base_val.is_a?(Hash) && override_val.is_a?(Hash)
      deep_merge(base_val, override_val)
    else
      override_val
    end
  end
end

Vagrant.configure("2") do |config|
  # Load default configuration
  defaults_file = File.expand_path("vm.json", File.dirname(__FILE__))
  default_config = JSON.parse(File.read(defaults_file))
  
  # Load project-specific configuration
  config_file = ENV['VM_CONFIG'] || File.expand_path("../../vm.json", File.dirname(__FILE__))
  if config_file && File.exist?(config_file)
    user_config = JSON.parse(File.read(config_file))
    # Merge user config with defaults
    project_config = deep_merge(default_config, user_config)
  else
    # Use defaults only
    project_config = default_config
  end
  
  # Extract configuration values
  project_name = project_config['project']['name']
  
  # Define a named machine instead of using default
  config.vm.define project_name do |machine|
    machine.vm.hostname = project_config['project']['hostname']
    machine.vm.box = project_config['vm']['box']
    vm_memory = project_config['vm']['memory']
    vm_cpus = project_config['vm']['cpus']
    vm_user = project_config['vm']['user']
    workspace_path = project_config['project']['workspace_path']
  
  # Validate critical configuration values
  errors = []
  
  if project_name.nil? || project_name.empty?
    errors << "project.name is required"
  end
  
  if workspace_path.nil? || workspace_path.empty?
    errors << "project.workspace_path is required"
  end
  
  if project_config['project']['hostname'].nil? || project_config['project']['hostname'].empty?
    errors << "project.hostname is required"
  end
  
  # Validate VM configuration exists
  unless project_config['vm']
    errors << "vm configuration section is required"
  else
    if project_config['vm']['box'].nil? || project_config['vm']['box'].empty?
      errors << "vm.box is required"
    end
    
    if project_config['vm']['memory'].nil? || project_config['vm']['memory'] <= 0
      errors << "vm.memory must be a positive number"
    end
    
    if project_config['vm']['cpus'].nil? || project_config['vm']['cpus'] <= 0
      errors << "vm.cpus must be a positive number"
    end
    
    if project_config['vm']['user'].nil? || project_config['vm']['user'].empty?
      errors << "vm.user is required"
    end
  end
  
  # Report all errors at once
  unless errors.empty?
    puts "\n" + "="*60
    puts "VM CONFIGURATION ERRORS:"
    puts "="*60
    errors.each_with_index do |error, index|
      puts "#{index + 1}. #{error}"
    end
    puts "\nConfiguration file: #{config_file}"
    puts "Please check your vm.json file and ensure all required fields are present."
    puts "="*60 + "\n"
    abort "VM configuration validation failed with #{errors.length} error(s)"
  end
  
    # Forward ports from config
    # Default to localhost-only for security (override with vm.port_binding)
    port_binding = project_config.dig('vm', 'port_binding') || "127.0.0.1"
    (project_config['ports'] || {}).each do |service, port|
      if port_binding == "0.0.0.0"
        # Explicitly bind to all interfaces if requested
        machine.vm.network "forwarded_port", guest: port, host: port, auto_correct: true
      else
        # Default: bind to specified IP (localhost by default)
        machine.vm.network "forwarded_port", guest: port, host: port, host_ip: port_binding, auto_correct: true
      end
    end
  
    # VirtualBox provider (default)
    machine.vm.provider "virtualbox" do |vb|
      # Set a clean VM name based on project
      vb.name = "#{project_name}-dev"
      
      vb.memory = vm_memory
      vb.cpus = vm_cpus
      vb.gui = false
    end
    
    # Parallels provider (alternative)
    machine.vm.provider "parallels" do |prl|
      # Set a clean VM name based on project
      prl.name = "#{project_name}-dev"
      
      prl.memory = vm_memory
      prl.cpus = vm_cpus
      
      # Ensure time synchronization
      prl.customize ["set", :id, "--time-sync", "on"]
    end
  
    # SSH configuration
    machine.ssh.forward_agent = true
    machine.ssh.forward_x11 = true
    machine.ssh.connect_timeout = 120
    machine.ssh.insert_key = true
  
    # Mount project root as workspace
    source_path = ENV['VM_PROJECT_DIR'] || "../.."
    machine.vm.synced_folder source_path, workspace_path
    
    # Mount vm tool directory for access to ansible files
    machine.vm.synced_folder File.dirname(__FILE__), "/vm-tool"
    
    machine.vm.synced_folder ".", "/vagrant", disabled: true
  
    # Write merged configuration for Ansible to use
    machine.vm.provision "shell", inline: <<-SHELL
      echo "Ensuring SSH service is running..."
      sudo systemctl enable ssh
      sudo systemctl start ssh
      
      echo "Temporarily disabling UFW for provisioning..."
      sudo ufw disable || true
      sudo ufw allow ssh || true
      
      echo "Ansible playbook available at /vm-tool/ansible/"
      
      # Write merged configuration for Ansible (separate from project vm.json)
      cat > /tmp/vm-config.json << 'EOF'
#{JSON.pretty_generate(project_config)}
EOF
    SHELL
    
    # Provision with Ansible
    machine.vm.provision "ansible_local" do |ansible|
      ansible.playbook = "playbook.yml"
      ansible.provisioning_path = "/vm-tool/ansible"
      ansible.install_mode = "pip"
      ansible.version = "latest"
    end
  end
end