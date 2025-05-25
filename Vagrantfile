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
  config.vm.hostname = project_config['project']['hostname']
  config.vm.box = project_config['vm']['box']
  vm_memory = project_config['vm']['memory']
  vm_cpus = project_config['vm']['cpus']
  vm_user = project_config['vm']['user']
  workspace_path = project_config['project']['workspace_path']
  
  # Forward ports from config
  # Default to localhost-only for security (override with vm.port_binding)
  port_binding = project_config.dig('vm', 'port_binding') || "127.0.0.1"
  project_config['ports'].each do |service, port|
    if port_binding == "0.0.0.0"
      # Explicitly bind to all interfaces if requested
      config.vm.network "forwarded_port", guest: port, host: port, auto_correct: true
    else
      # Default: bind to specified IP (localhost by default)
      config.vm.network "forwarded_port", guest: port, host: port, host_ip: port_binding, auto_correct: true
    end
  end
  
  # VirtualBox provider (default)
  config.vm.provider "virtualbox" do |vb|
    # Set a clean VM name based on project
    if config_file && File.exist?(config_file)
      vb.name = "#{JSON.parse(File.read(config_file))['project']['name']}-dev"
    else
      vb.name = "#{project_name}-dev"
    end
    
    vb.memory = vm_memory
    vb.cpus = vm_cpus
    vb.gui = false
  end
  
  # Parallels provider (alternative)
  config.vm.provider "parallels" do |prl|
    # Set a clean VM name based on project
    if config_file && File.exist?(config_file)
      prl.name = "#{JSON.parse(File.read(config_file))['project']['name']}-dev"
    else
      prl.name = "#{project_name}-dev"
    end
    
    prl.memory = vm_memory
    prl.cpus = vm_cpus
    
    # Ensure time synchronization
    prl.customize ["set", :id, "--time-sync", "on"]
  end
  
  # SSH configuration
  config.ssh.forward_agent = true
  config.ssh.forward_x11 = true
  config.ssh.connect_timeout = 120
  config.ssh.insert_key = true
  
  # Mount project root as workspace
  config.vm.synced_folder "../..", workspace_path
  config.vm.synced_folder ".", "/vagrant", disabled: true
  
  # Write merged configuration for Ansible to use
  config.vm.provision "shell", inline: <<-SHELL
    echo "Ensuring SSH service is running..."
    sudo systemctl enable ssh
    sudo systemctl start ssh
    
    echo "Temporarily disabling UFW for provisioning..."
    sudo ufw disable || true
    sudo ufw allow ssh || true
    
    # Write merged configuration for Ansible (separate from project vm.json)
    cat > /tmp/vm-config.json << 'EOF'
#{JSON.pretty_generate(project_config)}
EOF
  SHELL
  
  # Provision with Ansible
  config.vm.provision "ansible_local" do |ansible|
    ansible.playbook = "ansible/playbook.yml"
    ansible.provisioning_path = "/workspace/packages/vm"
    ansible.install_mode = "pip"
    ansible.version = "latest"
  end
end