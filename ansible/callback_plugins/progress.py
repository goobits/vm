#!/usr/bin/env python3
"""
Progress callback plugin for Ansible
Provides clean progress output for VM provisioning
"""

from ansible.plugins.callback import CallbackBase
import os
import sys

class CallbackModule(CallbackBase):
    CALLBACK_VERSION = 2.0
    CALLBACK_TYPE = 'stdout'
    CALLBACK_NAME = 'progress'

    def __init__(self):
        super(CallbackModule, self).__init__()
        self.task_status = {}
        self.current_task = None
        self.current_play = None
        self.task_count = 0
        self.total_tasks = 0
        self.system_tasks = []
        self.dev_tools_tasks = []
        self.final_setup_tasks = []
        
        # Task categorization
        self.task_categories = {
            'System Configuration': [
                'Set hostname',
                'Update /etc/hosts',
                'Generate en_US.UTF-8 locale',
                'Set system locale',
                'Set system timezone',
                'Update apt cache',
                'Install locale packages',
                'Install base system packages'
            ],
            'Development Tools': [
                'Install NVM',
                'Install Node.js',
                'Update npm',
                'Enable corepack',
                'Install pnpm',
                'Install global npm packages',
                'Install additional APT packages',
                'Install Rust',
                'Install cargo packages',
                'Install pyenv'
            ],
            'Final Setup': [
                'Change.*shell',
                'Generate .zshrc',
                'shell configuration',
                'permissions'
            ]
        }

    def v2_playbook_on_play_start(self, play):
        """Called when a play starts"""
        self.current_play = play.get_name()
        
    def v2_playbook_on_task_start(self, task, is_conditional):
        """Called when a task starts"""
        self.current_task = task.get_name()
        
        # Skip internal tasks
        if self.current_task.startswith('Gathering Facts') or not self.current_task:
            return
            
        # Categorize task
        category = self._categorize_task(self.current_task)
        
        # Print task with appropriate formatting
        if category:
            # We'll print category headers in v2_runner_on_ok
            pass
        else:
            # Regular task output
            sys.stdout.write(f"   ├─ {self.current_task} ")
            sys.stdout.flush()

    def v2_runner_on_ok(self, result):
        """Called when a task succeeds"""
        if self.current_task and not self.current_task.startswith('Gathering Facts'):
            task_name = self.current_task
            
            # Check if task was changed
            changed = result._result.get('changed', False)
            
            # Determine status symbol
            if changed:
                status = "✓"
            else:
                status = "✓"  # Same symbol but could differentiate if needed
                
            # Get category for proper indentation
            category = self._categorize_task(task_name)
            
            if category:
                # For categorized tasks, show simplified output
                simple_name = self._simplify_task_name(task_name)
                sys.stdout.write(f"\r   │  ├─ {simple_name} {'.' * (35 - len(simple_name))} ✓\n")
            else:
                # Clear the line and show completion
                sys.stdout.write(f"\r   ├─ {task_name} {'.' * (50 - len(task_name))} ✓\n")
            sys.stdout.flush()

    def v2_runner_on_failed(self, result, ignore_errors=False):
        """Called when a task fails"""
        if self.current_task and not ignore_errors:
            sys.stdout.write(f"\r   ├─ {self.current_task} {'.' * (50 - len(self.current_task))} ✗\n")
            sys.stdout.write(f"   │  └─ Error: {result._result.get('msg', 'Unknown error')}\n")
            sys.stdout.flush()

    def v2_runner_on_skipped(self, result):
        """Called when a task is skipped"""
        # Don't show skipped tasks in progress mode
        pass

    def v2_playbook_on_stats(self, stats):
        """Called at the end of the playbook"""
        # Final status is handled by the shell script
        pass

    def _categorize_task(self, task_name):
        """Categorize a task based on its name"""
        for category, patterns in self.task_categories.items():
            for pattern in patterns:
                if pattern.lower() in task_name.lower() or \
                   (pattern.startswith('.*') and pattern.endswith('.*') and 
                    pattern[2:-2].lower() in task_name.lower()):
                    return category
        return None

    def _simplify_task_name(self, task_name):
        """Simplify task name for cleaner output"""
        # Remove common prefixes
        simplifications = {
            'Install ': '',
            'Set ': '',
            'Update ': '',
            'Enable ': '',
            'Generate ': '',
            'Download ': '',
            'Change ': ''
        }
        
        simple_name = task_name
        for prefix, replacement in simplifications.items():
            if simple_name.startswith(prefix):
                simple_name = simple_name[len(prefix):]
                break
                
        # Specific simplifications
        if 'base system packages' in simple_name:
            simple_name = 'Base packages (25)'
        elif 'global npm packages' in simple_name:
            simple_name = 'Global packages (5)'
        elif 'additional APT packages' in simple_name:
            simple_name = 'Additional packages'
            
        return simple_name

    def display(self, msg, color=None, stderr=False, screen_only=False, log_only=False, newline=True):
        """Override display to control output"""
        # Suppress default output in progress mode
        pass