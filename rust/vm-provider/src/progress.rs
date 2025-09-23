use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use regex::Regex;
use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::Arc;
use std::time::Duration;

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
            step_regex: Regex::new(r"Step (\d+)/(\d+)").unwrap_or_else(|_| {
                // If the primary regex fails, create a never-matching regex
                // This should never fail as it's a simple pattern
                Regex::new(r"(?-u)a^").expect("Failed to create fallback regex")
            }),
            layer_pull_regex: Regex::new(r"([a-f0-9]{12}): Pulling fs layer").unwrap_or_else(
                |_| {
                    // If the primary regex fails, create a never-matching regex
                    // This should never fail as it's a simple pattern
                    Regex::new(r"(?-u)a^").expect("Failed to create fallback regex")
                },
            ),
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
        println!("{} {}", icon, phase);
    }

    pub fn subtask(connector: &str, task: &str) {
        println!("{} {}", connector, task);
    }

    pub fn complete(connector: &str, message: &str) {
        println!("{} ✅ {}", connector, message);
    }

    pub fn warning(connector: &str, message: &str) {
        println!("{} ⚠️ {}", connector, message);
    }

    pub fn error(connector: &str, message: &str) {
        println!("{} ❌ {}", connector, message);
    }

    pub fn error_with_details(connector: &str, main_message: &str, details: &[&str]) {
        println!("{} ❌ {}", connector, main_message);
        for detail in details {
            println!("     └─ {}", detail);
        }
    }

    pub fn error_with_hint(connector: &str, message: &str, hint: &str) {
        println!("{} ❌ {}", connector, message);
        println!("     💡 {}", hint);
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
