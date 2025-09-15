use std::env;
use vm_detector::{detect_project_type, format_detected_types};

fn main() {
    let project_dir = env::current_dir().expect("Failed to get current directory");
    let detected_types = detect_project_type(&project_dir);
    let formatted = format_detected_types(detected_types);
    println!("{}", formatted);
}
