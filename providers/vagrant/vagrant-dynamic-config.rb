# -*- mode: ruby -*-
# vi: set ft=ruby :
# Vagrant Temporary VM Configuration Generator
# Purpose: Generate dynamic Vagrantfile for temp VMs with custom mounts
# Part of: Docker-Vagrant Parity Enhancement (Phase 2A)

require 'json'
require 'yaml'

# Configuration generator for Vagrant temp VMs
class VagrantTempConfig
  def initialize(vm_name, mounts = [], project_dir = nil)
    @vm_name = vm_name
    @mounts = mounts
    @project_dir = project_dir || Dir.pwd
    @config = load_base_config
  end

  # Load base configuration from schema defaults
  def load_base_config
    # Try to load from shared config processor first
    config_processor = File.expand_path("../../shared/config-processor.sh", File.dirname(__FILE__))
    
    if File.exist?(config_processor)
      begin
        # Use the config processor to get defaults
        config_json = `"#{config_processor}" load-config "__SCAN__" "#{@project_dir}" 2>/dev/null`
        if $?.exitstatus == 0
          return JSON.parse(config_json)
        end
      rescue => e
        # Fall back to minimal config if config processor fails
      end
    end
    
    # Minimal fallback configuration for temp VMs
    {
      'project' => {
        'name' => @vm_name,
        'hostname' => "#{@vm_name}.local",
        'workspace_path' => '/workspace'
      },
      'vm' => {
        'box' => 'ubuntu/jammy64',
        'memory' => 2048,
        'cpus' => 2,
        'user' => 'developer'
      },
      'provider' => 'vagrant',
      'services' => {
        'postgresql' => { 'enabled' => false },
        'redis' => { 'enabled' => false },
        'mongodb' => { 'enabled' => false }
      },
      'ports' => {}
    }
  end

  # Generate Vagrantfile content for temp VM
  def generate_vagrantfile
    <<~VAGRANTFILE
      # -*- mode: ruby -*-
      # vi: set ft=ruby :
      # Auto-generated Vagrantfile for temp VM: #{@vm_name}
      
      Vagrant.configure("2") do |config|
        # Define temp VM machine
        config.vm.define "#{@vm_name}" do |machine|
          machine.vm.hostname = "#{@config['project']['hostname']}"
          machine.vm.box = "#{@config['vm']['box']}"
          
          # Disable default /vagrant mount since this is a temp VM
          machine.vm.synced_folder ".", "/vagrant", disabled: true
          
          # Mount vm-tool directory for access to ansible files
          machine.vm.synced_folder "#{File.expand_path("../..", File.dirname(__FILE__))}", "/vm-tool"
          
          #{generate_mount_config}
          
          # VirtualBox provider configuration
          machine.vm.provider "virtualbox" do |vb|
            vb.name = "#{@vm_name}"
            vb.memory = #{@config['vm']['memory']}
            vb.cpus = #{@config['vm']['cpus']}
            vb.gui = false
            
            # Optimize for temp VM performance
            vb.customize ["modifyvm", :id, "--natdnshostresolver1", "on"]
            vb.customize ["modifyvm", :id, "--natdnsproxy1", "on"]
          end
          
          # Parallels provider configuration
          machine.vm.provider "parallels" do |prl|
            prl.name = "#{@vm_name}"
            prl.memory = #{@config['vm']['memory']}
            prl.cpus = #{@config['vm']['cpus']}
            
            # Optimize for performance
            prl.customize ["set", :id, "--time-sync", "on"]
          end
          
          # SSH configuration
          machine.ssh.forward_agent = true
          machine.ssh.forward_x11 = false  # Disable for temp VMs unless needed
          machine.ssh.connect_timeout = 60  # Shorter timeout for temp VMs
          machine.ssh.insert_key = true
          
          #{generate_provision_config}
        end
      end
    VAGRANTFILE
  end

  # Generate mount configuration for requested directories
  def generate_mount_config
    return "" if @mounts.empty?
    
    mount_configs = []
    
    @mounts.each do |mount|
      # Parse mount string: source:target:permissions or source:permissions
      parts = mount.split(':')
      source = parts[0]
      permissions = parts[-1] == 'ro' || parts[-1] == 'rw' ? parts[-1] : 'rw'
      
      # Get absolute source path
      abs_source = File.expand_path(source, @project_dir)
      
      # Generate target path in workspace
      target_name = File.basename(abs_source)
      target = "#{@config['project']['workspace_path']}/#{target_name}"
      
      # Configure synced folder based on permissions
      if permissions == 'ro'
        # Read-only mount using rsync for better performance
        mount_configs << <<~MOUNT.strip
          # Read-only mount: #{abs_source} -> #{target}
          machine.vm.synced_folder "#{abs_source}", "#{target}",
            type: "rsync",
            rsync__args: ["--verbose", "--archive", "--delete", "--compress"],
            rsync__exclude: [".git/", "node_modules/", ".DS_Store"],
            create: true
        MOUNT
      else
        # Read-write mount using default provider
        mount_configs << <<~MOUNT.strip
          # Read-write mount: #{abs_source} -> #{target}
          machine.vm.synced_folder "#{abs_source}", "#{target}",
            create: true
        MOUNT
      end
    end
    
    mount_configs.join("\n          \n          ")
  end

  # Generate provisioning configuration
  def generate_provision_config
    vm_user = @config['vm']['user']
    
    <<~PROVISION
      # Minimal provisioning for temp VM
      machine.vm.provision "shell", inline: <<-SHELL
        echo "Setting up temp VM: #{@vm_name}"
        
        # Ensure SSH service is running
        sudo systemctl enable ssh
        sudo systemctl start ssh
        
        # Create VM user if it doesn't exist
        if ! id "#{vm_user}" &>/dev/null; then
          sudo useradd -m -s /bin/bash "#{vm_user}"
          sudo usermod -aG sudo "#{vm_user}"
          echo "#{vm_user} ALL=(ALL) NOPASSWD:ALL" | sudo tee /etc/sudoers.d/#{vm_user}
        fi
        
        # Ensure workspace directory exists and has correct permissions
        sudo mkdir -p "#{@config['project']['workspace_path']}"
        sudo chown "#{vm_user}:#{vm_user}" "#{@config['project']['workspace_path']}"
        
        # Basic development tools for temp VM
        sudo apt-get update -qq
        sudo apt-get install -y -qq curl wget git build-essential
        
        # Mark provisioning as complete
        sudo touch /tmp/provisioning_complete
        sudo chown "#{vm_user}:#{vm_user}" /tmp/provisioning_complete
        
        echo "Temp VM #{@vm_name} setup complete"
      SHELL
      
      # Set ownership after synced folders are mounted
      machine.vm.provision "shell", run: "always", inline: <<-SHELL
        # Fix ownership of mounted directories
        sudo chown -R "#{vm_user}:#{vm_user}" "#{@config['project']['workspace_path']}" 2>/dev/null || true
      SHELL
    PROVISION
  end

  # Write Vagrantfile to specified directory
  def write_to(directory)
    vagrantfile_path = File.join(directory, 'Vagrantfile')
    File.write(vagrantfile_path, generate_vagrantfile)
    vagrantfile_path
  end

  # Get VM name
  def vm_name
    @vm_name
  end

  # Get mount information for state saving
  def mount_info
    @mounts.map do |mount|
      parts = mount.split(':')
      source = parts[0]
      permissions = parts[-1] == 'ro' || parts[-1] == 'rw' ? parts[-1] : 'rw'
      abs_source = File.expand_path(source, @project_dir)
      target_name = File.basename(abs_source)
      target = "#{@config['project']['workspace_path']}/#{target_name}"
      
      {
        'source' => abs_source,
        'target' => target,
        'permissions' => permissions
      }
    end
  end
end

# Command-line interface for testing
if __FILE__ == $0
  if ARGV.length < 1
    puts "Usage: ruby vagrant-dynamic-config.rb <vm-name> [mount1] [mount2] ..."
    puts "Example: ruby vagrant-dynamic-config.rb vmtemp-123 ./src:rw ./docs:ro"
    exit 1
  end
  
  vm_name = ARGV[0]
  mounts = ARGV[1..-1] || []
  project_dir = Dir.pwd
  
  config = VagrantTempConfig.new(vm_name, mounts, project_dir)
  puts config.generate_vagrantfile
end