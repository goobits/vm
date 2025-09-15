use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::time::Duration;
use std::io::{self, Write};

pub struct ProgressReporter {
    mp: MultiProgress,
    style: ProgressStyle,
}

impl ProgressReporter {
    pub fn new() -> Self {
        let mp = MultiProgress::new();
        let style = ProgressStyle::default_spinner()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", "✅"])
            .template("{spinner:.green} {prefix:.bold} {wide_msg}").unwrap();

        Self { mp, style }
    }

    pub fn start_phase(&self, name: &str) -> ProgressBar {
        let pb = self.mp.add(ProgressBar::new_spinner());
        pb.set_style(self.style.clone());
        pb.set_prefix(format!("[Phase] {}", name));
        pb.enable_steady_tick(Duration::from_millis(100));
        pb
    }

    pub fn task(&self, phase_pb: &ProgressBar, msg: &str) {
        phase_pb.set_message(format!("- {}", msg));
    }

    pub fn finish_phase(&self, pb: &ProgressBar, msg: &str) {
        pb.finish_with_message(format!("{} {}", pb.message(), msg));
    }

    /// Display a phase header with icon (e.g., "🔧 BUILD PHASE")
    pub fn phase_header(&self, icon: &str, phase: &str) {
        println!("{} {}", icon, phase);
    }

    /// Display a subtask with tree structure (├─ or └─)
    pub fn subtask(&self, connector: &str, task: &str) {
        println!("{} {}", connector, task);
    }

    /// Display a completion message with checkmark
    pub fn complete(&self, connector: &str, message: &str) {
        println!("{} ✅ {}", connector, message);
    }

    /// Display a warning message
    pub fn warning(&self, connector: &str, message: &str) {
        println!("{} ⚠️ {}", connector, message);
    }

    /// Display an error message
    pub fn error(&self, connector: &str, message: &str) {
        println!("{} ❌ {}", connector, message);
    }
}

/// Simple status formatter for VM status output
pub struct StatusFormatter;

impl StatusFormatter {
    pub fn new() -> Self {
        Self
    }

    pub fn format_status(&self, vm_name: &str, state: &str, provider: &str, memory: Option<u32>, cpus: Option<u32>) {
        println!("VM Status Report");
        println!("================");
        println!("Name: {}", vm_name);

        let status_icon = match state.to_lowercase().as_str() {
            "running" => "🟢 Running",
            "stopped" | "exited" => "🔴 Stopped",
            _ => "⚫ Not Found",
        };
        println!("Status: {}", status_icon);
        println!("Provider: {}", provider);

        if let Some(mem) = memory {
            println!("Memory: {} MB", mem);
        }

        if let Some(cpu) = cpus {
            println!("CPUs: {}", cpu);
        }
    }
}

/// Prompt user for confirmation with a yes/no question
pub fn confirm_prompt(message: &str) -> bool {
    print!("{}", message);
    io::stdout().flush().unwrap();

    let mut input = String::new();
    match io::stdin().read_line(&mut input) {
        Ok(_) => {
            let response = input.trim().to_lowercase();
            matches!(response.as_str(), "y" | "yes")
        }
        Err(_) => false,
    }
}
