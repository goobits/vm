use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use regex::Regex;
use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::info;
use vm_cli::msg;
use vm_core::vm_println;
use vm_messages::messages::MESSAGES;

// --- Generic Progress Parsing --- //

/// A generic trait for parsing command output to drive a progress bar.
pub trait ProgressParser: Send + Sync {
    /// Parses a single line of output.
    fn parse_line(&mut self, line: &str);
    /// Marks the progress as finished.
    fn finish(&self);
}

// --- Docker-specific Progress Parser --- //

/// A progress parser specifically for `docker build` output.
pub struct DockerProgressParser {
    mp: Arc<MultiProgress>,
    main_bar: ProgressBar,
    step_regex: Regex,
    layer_pull_regex: Regex,
    total_steps: u32,
    current_step: u32,
    layer_bars: HashMap<String, ProgressBar>,
}

impl DockerProgressParser {
    pub fn new() -> Self {
        let mp = Arc::new(MultiProgress::new());
        let main_bar = mp.add(ProgressBar::new(0));
        main_bar.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
                )
                .unwrap_or_else(|_| ProgressStyle::default_bar())
                .progress_chars("#>-"),
        );

        Self {
            mp,
            main_bar,
            step_regex: Regex::new(r"Step (\d+)/(\d+)")
                .expect("Hardcoded Docker step regex pattern should always compile"),
            layer_pull_regex: Regex::new(r"([a-f0-9]{12}): Pulling fs layer")
                .expect("Hardcoded Docker layer pull regex pattern should always compile"),
            total_steps: 0,
            current_step: 0,
            layer_bars: HashMap::new(),
        }
    }
}

impl Default for DockerProgressParser {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressParser for DockerProgressParser {
    fn parse_line(&mut self, line: &str) {
        if let Some(caps) = self.step_regex.captures(line) {
            let step: u32 = caps
                .get(1)
                .and_then(|m| m.as_str().parse().ok())
                .unwrap_or(0);
            let total: u32 = caps
                .get(2)
                .and_then(|m| m.as_str().parse().ok())
                .unwrap_or(0);
            if self.total_steps == 0 {
                self.total_steps = total;
                self.main_bar.set_length(self.total_steps as u64);
            }
            self.current_step = step;
            self.main_bar.set_position(self.current_step as u64);
            self.main_bar.set_message(line.trim().to_string());
        }

        if let Some(caps) = self.layer_pull_regex.captures(line) {
            if let Some(layer_id_match) = caps.get(1) {
                let layer_id = layer_id_match.as_str().to_string();
                if !self.layer_bars.contains_key(&layer_id) {
                    let pb = self.mp.add(ProgressBar::new_spinner());
                    pb.set_style(
                        ProgressStyle::default_spinner()
                            .template("  {prefix:12} {spinner} {wide_msg}")
                            .unwrap_or_else(|_| ProgressStyle::default_spinner()),
                    );
                    pb.set_prefix(layer_id.clone());
                    pb.set_message("Pulling...");
                    self.layer_bars.insert(layer_id, pb);
                }
            }
        }
    }

    fn finish(&self) {
        self.main_bar.finish_with_message("Build complete");
        for bar in self.layer_bars.values() {
            bar.finish_and_clear();
        }
    }
}

// --- Ansible Progress Parser --- //

/// Progress tracking for Ansible playbook execution
#[derive(Clone)]
pub struct AnsibleProgressParser {
    tasks: Arc<Mutex<Vec<TaskProgress>>>,
    current_task: Arc<Mutex<Option<String>>>,
    show_output: bool,
}

#[derive(Debug, Clone)]
struct TaskProgress {
    name: String,
    status: TaskStatus,
    subtasks: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

impl AnsibleProgressParser {
    pub fn new(show_output: bool) -> Self {
        Self {
            tasks: Arc::new(Mutex::new(Vec::new())),
            current_task: Arc::new(Mutex::new(None)),
            show_output,
        }
    }

    fn update_display(&self) {
        if self.show_output {
            return; // In verbose mode, don't show progress
        }

        // Clear screen and redraw
        print!("\x1B[2J\x1B[1;1H"); // Clear screen and move to top
        vm_println!("{}", MESSAGES.progress_creating_vm);

        let tasks = self.tasks.lock().expect("Mutex should not be poisoned");
        for task in tasks.iter() {
            let icon = match task.status {
                TaskStatus::Completed => "  ✓",
                TaskStatus::Running => "  ⠴",
                TaskStatus::Failed => "  ✗",
                TaskStatus::Skipped => "  -",
                TaskStatus::Pending => "  ○",
            };

            println!("{} {}", icon, task.name);

            // Show subtasks for running task
            if task.status == TaskStatus::Running && !task.subtasks.is_empty() {
                for subtask in &task.subtasks {
                    println!("      {subtask}");
                }
            }
        }

        let _ = io::stdout().flush();
    }

    fn extract_task_name(line: &str) -> Option<String> {
        // Parse TASK [task name] format
        if line.starts_with("TASK [") {
            if let Some(end) = line.find(']') {
                let name = line[6..end].to_string();
                return Some(name);
            }
        }
        None
    }
}

impl ProgressParser for AnsibleProgressParser {
    fn parse_line(&mut self, line: &str) {
        if self.show_output {
            info!("{}", line); // In verbose mode, show everything
            return;
        }

        // Detect new task
        if let Some(task_name) = Self::extract_task_name(line) {
            let mut tasks = self.tasks.lock().expect("Mutex should not be poisoned");

            // Mark previous task as completed
            if let Some(last_task) = tasks.last_mut() {
                if last_task.status == TaskStatus::Running {
                    last_task.status = TaskStatus::Completed;
                }
            }

            // Add new task
            tasks.push(TaskProgress {
                name: task_name.clone(),
                status: TaskStatus::Running,
                subtasks: Vec::new(),
            });

            *self
                .current_task
                .lock()
                .expect("Mutex should not be poisoned") = Some(task_name);
            drop(tasks);
            self.update_display();
        }
        // Detect task completion
        else if line.contains("ok:") || line.contains("changed:") {
            let mut tasks = self.tasks.lock().expect("Mutex should not be poisoned");
            if let Some(last_task) = tasks.last_mut() {
                if last_task.status == TaskStatus::Running {
                    last_task.status = TaskStatus::Completed;
                }
            }
            drop(tasks);
            self.update_display();
        }
        // Detect skipped task
        else if line.contains("skipping:") {
            let mut tasks = self.tasks.lock().expect("Mutex should not be poisoned");
            if let Some(last_task) = tasks.last_mut() {
                last_task.status = TaskStatus::Skipped;
            }
            drop(tasks);
            self.update_display();
        }
        // Detect failed task
        else if line.contains("failed:") || line.contains("FAILED") {
            let mut tasks = self.tasks.lock().expect("Mutex should not be poisoned");
            if let Some(last_task) = tasks.last_mut() {
                last_task.status = TaskStatus::Failed;
            }
            drop(tasks);

            // Show error in full
            vm_println!("{}", msg!(MESSAGES.progress_ansible_error, error = line));
            self.update_display();
        }
        // Track package installations
        else if line.contains("Installing") || line.contains("Downloading") {
            let mut tasks = self.tasks.lock().expect("Mutex should not be poisoned");
            if let Some(last_task) = tasks.last_mut() {
                if last_task.status == TaskStatus::Running {
                    // Extract package info if possible
                    #[allow(clippy::excessive_nesting)]
                    if let Some(pkg_info) = line.split_whitespace().nth(1) {
                        last_task.subtasks.push(format!("Installing {pkg_info}"));
                        // Keep only last 3 subtasks
                        if last_task.subtasks.len() > 3 {
                            last_task.subtasks.remove(0);
                        }
                    }
                }
            }
            drop(tasks);
            self.update_display();
        }
    }

    fn finish(&self) {
        if !self.show_output {
            let mut tasks = self.tasks.lock().expect("Mutex should not be poisoned");
            // Mark any remaining running tasks as completed
            for task in tasks.iter_mut() {
                if task.status == TaskStatus::Running {
                    task.status = TaskStatus::Completed;
                }
            }
            drop(tasks);
            self.update_display();
            vm_println!("{}", MESSAGES.progress_provisioning_complete);
        }
    }
}

// --- Existing Progress Reporter --- //

pub struct ProgressReporter {
    mp: MultiProgress,
    style: ProgressStyle,
}

impl Default for ProgressReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressReporter {
    pub fn new() -> Self {
        let mp = MultiProgress::new();
        let style = ProgressStyle::default_spinner()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", "✅"])
            .template("{spinner:.green} {prefix:.bold} {wide_msg}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner());

        Self { mp, style }
    }

    pub fn start_phase(&self, name: &str) -> ProgressBar {
        let pb = self.mp.add(ProgressBar::new_spinner());
        pb.set_style(self.style.clone());
        pb.set_prefix(format!("[Phase] {name}"));
        pb.enable_steady_tick(Duration::from_millis(100));
        pb
    }

    pub fn task(phase_pb: &ProgressBar, msg: &str) {
        phase_pb.set_message(format!("- {msg}"));
    }

    pub fn finish_phase(pb: &ProgressBar, msg: &str) {
        pb.finish_with_message(format!("{} {}", pb.message(), msg));
    }

    pub fn phase_header(icon: &str, phase: &str) {
        vm_println!(
            "{}",
            msg!(MESSAGES.progress_phase_header, icon = icon, phase = phase)
        );
    }

    pub fn subtask(connector: &str, task: &str) {
        vm_println!(
            "{}",
            msg!(
                MESSAGES.progress_subtask,
                connector = connector,
                task = task
            )
        );
    }

    pub fn complete(connector: &str, message: &str) {
        vm_println!(
            "{}",
            msg!(
                MESSAGES.progress_complete,
                connector = connector,
                message = message
            )
        );
    }

    pub fn warning(connector: &str, message: &str) {
        vm_println!(
            "{}",
            msg!(
                MESSAGES.progress_warning,
                connector = connector,
                message = message
            )
        );
    }

    pub fn error(connector: &str, message: &str) {
        vm_println!(
            "{}",
            msg!(
                MESSAGES.progress_error,
                connector = connector,
                message = message
            )
        );
    }

    pub fn error_with_details(connector: &str, main_message: &str, details: &[&str]) {
        vm_println!(
            "{}",
            msg!(
                MESSAGES.progress_error,
                connector = connector,
                message = main_message
            )
        );
        for detail in details {
            vm_println!("{}", msg!(MESSAGES.progress_error_detail, detail = *detail));
        }
    }

    pub fn error_with_hint(connector: &str, message: &str, hint: &str) {
        vm_println!(
            "{}",
            msg!(
                MESSAGES.progress_error,
                connector = connector,
                message = message
            )
        );
        vm_println!("{}", msg!(MESSAGES.progress_error_hint, hint = hint));
    }
}

// --- Other Utilities --- //

/// Simple status formatter for VM status output
pub struct StatusFormatter;

impl Default for StatusFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusFormatter {
    pub fn new() -> Self {
        Self
    }

    pub fn format_status(
        vm_name: &str,
        state: &str,
        provider: &str,
        memory: Option<u32>,
        cpus: Option<u32>,
    ) {
        vm_println!("{}", MESSAGES.status_report_header);
        vm_println!("{}", MESSAGES.status_report_separator);
        vm_println!("{}", msg!(MESSAGES.status_report_name, name = vm_name));

        let status_icon = match state.to_lowercase().as_str() {
            "running" => "🟢 Running",
            "stopped" | "exited" => "🔴 Stopped",
            _ => "⚫ Not Found",
        };
        vm_println!(
            "{}",
            msg!(MESSAGES.status_report_status, status = status_icon)
        );
        vm_println!(
            "{}",
            msg!(MESSAGES.status_report_provider, provider = provider)
        );

        if let Some(mem) = memory {
            vm_println!(
                "{}",
                msg!(MESSAGES.status_report_memory, memory = mem.to_string())
            );
        }

        if let Some(cpu) = cpus {
            vm_println!(
                "{}",
                msg!(MESSAGES.status_report_cpus, cpus = cpu.to_string())
            );
        }
    }
}

/// Prompt user for confirmation with a yes/no question
pub fn confirm_prompt(message: &str) -> bool {
    print!("{message}");
    let _ = io::stdout().flush();

    let mut input = String::new();
    match io::stdin().read_line(&mut input) {
        Ok(_) => {
            let response = input.trim().to_lowercase();
            matches!(response.as_str(), "y" | "yes")
        }
        Err(_) => false,
    }
}
