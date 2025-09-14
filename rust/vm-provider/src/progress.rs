use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::time::Duration;

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
}
