pub(super) fn port_conflicts() -> Vec<u16> {
    [3000, 5432, 6379, 8080, 27017, 3306]
        .into_iter()
        .filter(|port| std::net::TcpListener::bind(("127.0.0.1", *port)).is_err())
        .collect()
}

#[cfg(target_os = "macos")]
pub(super) fn file_descriptor_usage() -> Option<(u64, u64)> {
    let output = std::process::Command::new("/usr/sbin/sysctl")
        .args(["-n", "kern.num_files", "kern.maxfiles"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| parse_descriptor_values(&String::from_utf8_lossy(&output.stdout)))?
}

#[cfg(target_os = "linux")]
pub(super) fn file_descriptor_usage() -> Option<(u64, u64)> {
    let values = std::fs::read_to_string("/proc/sys/fs/file-nr").ok()?;
    parse_linux_descriptor_values(&values)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(super) fn file_descriptor_usage() -> Option<(u64, u64)> {
    None
}

#[cfg(any(target_os = "macos", test))]
fn parse_descriptor_values(values: &str) -> Option<(u64, u64)> {
    let mut values = values
        .split_whitespace()
        .filter_map(|value| value.parse().ok());
    Some((values.next()?, values.next()?))
}

#[cfg(target_os = "linux")]
fn parse_linux_descriptor_values(values: &str) -> Option<(u64, u64)> {
    let mut values = values
        .split_whitespace()
        .filter_map(|value| value.parse::<u64>().ok());
    let allocated = values.next()?;
    let unused = values.next()?;
    let limit = values.next()?;
    Some((allocated.saturating_sub(unused), limit))
}

#[cfg(test)]
mod tests {
    use super::parse_descriptor_values;

    #[test]
    fn parses_macos_file_descriptor_counters() {
        assert_eq!(
            parse_descriptor_values("8000\n10000\n"),
            Some((8000, 10000))
        );
        assert_eq!(parse_descriptor_values("unknown"), None);
    }
}
