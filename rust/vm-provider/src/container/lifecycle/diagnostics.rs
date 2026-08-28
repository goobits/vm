//! Best-effort container storage and process evidence for targeted status checks.

use std::process::Command;

use super::LifecycleOperations;
use crate::container::artifacts::compose_path;
use crate::{MountUsage, RuntimeDiagnostics};

impl<'a> LifecycleOperations<'a> {
    pub(super) fn collect_runtime_diagnostics(
        &self,
        container_name: &str,
        container_info: &serde_json::Value,
        is_running: bool,
    ) -> RuntimeDiagnostics {
        let instance = self
            .resolve_instance_name_for_target(Some(container_name))
            .ok()
            .flatten();
        let generated_config = compose_path(self.generated_dir, instance.as_deref());
        let (writable_layer_bytes, root_filesystem_bytes) =
            self.inspect_layer_sizes(container_name);
        let cgroup = if is_running {
            self.read_cgroup_metrics(container_name)
        } else {
            None
        };
        let host_config = &container_info["HostConfig"];
        let mut mounts = self.mount_usage(container_name, container_info, is_running);
        self.add_tmp_usage(container_name, host_config, is_running, &mut mounts);

        RuntimeDiagnostics {
            generated_config_exists: generated_config.is_file(),
            generated_config: Some(generated_config),
            writable_layer_bytes,
            root_filesystem_bytes,
            memory_peak_bytes: cgroup.as_ref().and_then(|metrics| metrics.memory_peak),
            pids_current: cgroup.as_ref().and_then(|metrics| metrics.pids_current),
            pids_peak: cgroup.as_ref().and_then(|metrics| metrics.pids_peak),
            pids_limit: cgroup
                .as_ref()
                .and_then(|metrics| metrics.pids_limit)
                .or_else(|| positive_u64(&host_config["PidsLimit"])),
            mounts,
            logging_driver: string_value(&host_config["LogConfig"]["Type"]),
            logging_options: string_pairs(&host_config["LogConfig"]["Config"]),
            restart_policy: string_value(&host_config["RestartPolicy"]["Name"]),
            stop_timeout_seconds: positive_u64(&host_config["StopTimeout"])
                .or_else(|| positive_u64(&container_info["Config"]["StopTimeout"]))
                .or_else(|| positive_u64(&container_info["StopTimeout"])),
        }
    }

    fn inspect_layer_sizes(&self, container_name: &str) -> (Option<u64>, Option<u64>) {
        let Some(info) = command_json(
            self.runtime.executable(),
            &["inspect", "--type", "container", "--size", container_name],
        )
        .and_then(|value| value.get(0).cloned()) else {
            return (None, None);
        };

        (info["SizeRw"].as_u64(), info["SizeRootFs"].as_u64())
    }

    fn read_cgroup_metrics(&self, container_name: &str) -> Option<CgroupMetrics> {
        const SCRIPT: &str = r#"for key in memory.peak pids.current pids.peak pids.max; do
  file="/sys/fs/cgroup/$key"
  if [ -r "$file" ]; then printf '%s=' "$key"; cat "$file"; fi
done"#;
        let output = Command::new(self.runtime.executable())
            .args(["exec", container_name, "sh", "-c", SCRIPT])
            .output()
            .ok()?;
        output.status.success().then_some(())?;
        Some(parse_cgroup_metrics(&String::from_utf8_lossy(
            &output.stdout,
        )))
    }

    fn mount_usage(
        &self,
        container_name: &str,
        container_info: &serde_json::Value,
        is_running: bool,
    ) -> Vec<MountUsage> {
        let mut mounts = container_info["Mounts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|mount| {
                let target = mount["Destination"].as_str()?.to_string();
                let storage_type = mount["Type"].as_str()?.to_string();
                let used_bytes = if is_running && storage_type == "volume" {
                    self.directory_size(container_name, &target)
                } else {
                    None
                };

                Some(MountUsage {
                    target,
                    storage_type,
                    name: string_value(&mount["Name"]),
                    used_bytes,
                    ..Default::default()
                })
            })
            .collect::<Vec<_>>();
        mounts.sort_by(|left, right| left.target.cmp(&right.target));
        mounts
    }

    fn add_tmp_usage(
        &self,
        container_name: &str,
        host_config: &serde_json::Value,
        is_running: bool,
        mounts: &mut Vec<MountUsage>,
    ) {
        let tmpfs_options = host_config["Tmpfs"]
            .as_object()
            .and_then(|tmpfs| tmpfs.get("/tmp"))
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string);
        let existing_tmpfs = mounts
            .iter()
            .any(|mount| mount.target == "/tmp" && mount.storage_type == "tmpfs");
        let is_tmpfs = tmpfs_options.is_some() || existing_tmpfs;
        let (capacity_bytes, filesystem_used) = if is_running && is_tmpfs {
            self.filesystem_usage(container_name, "/tmp")
                .unwrap_or_default()
        } else {
            (None, None)
        };
        let used_bytes = if is_running && !is_tmpfs {
            self.directory_size(container_name, "/tmp")
        } else {
            filesystem_used
        };

        if let Some(existing) = mounts.iter_mut().find(|mount| mount.target == "/tmp") {
            existing.capacity_bytes = capacity_bytes;
            existing.used_bytes = used_bytes;
            existing.options = tmpfs_options;
            return;
        }

        mounts.push(MountUsage {
            target: "/tmp".to_string(),
            storage_type: if is_tmpfs {
                "tmpfs".to_string()
            } else {
                "writable-layer".to_string()
            },
            used_bytes,
            capacity_bytes,
            options: tmpfs_options,
            ..Default::default()
        });
        mounts.sort_by(|left, right| left.target.cmp(&right.target));
    }

    fn filesystem_usage(
        &self,
        container_name: &str,
        target: &str,
    ) -> Option<(Option<u64>, Option<u64>)> {
        let output = Command::new(self.runtime.executable())
            .args(["exec", container_name, "df", "-kP", target])
            .output()
            .ok()?;
        output.status.success().then_some(())?;
        Some(parse_df_bytes(&String::from_utf8_lossy(&output.stdout)))
    }

    fn directory_size(&self, container_name: &str, target: &str) -> Option<u64> {
        let output = Command::new(self.runtime.executable())
            .args(["exec", container_name, "du", "-sk", target])
            .output()
            .ok()?;
        output.status.success().then_some(())?;
        String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()
            .map(|kibibytes| kibibytes.saturating_mul(1024))
    }
}

#[derive(Debug, Default)]
struct CgroupMetrics {
    memory_peak: Option<u64>,
    pids_current: Option<u64>,
    pids_peak: Option<u64>,
    pids_limit: Option<u64>,
}

fn command_json(executable: &str, args: &[&str]) -> Option<serde_json::Value> {
    let output = Command::new(executable).args(args).output().ok()?;
    output.status.success().then_some(())?;
    serde_json::from_slice(&output.stdout).ok()
}

fn parse_cgroup_metrics(output: &str) -> CgroupMetrics {
    let mut metrics = CgroupMetrics::default();
    for (key, value) in output.lines().filter_map(|line| line.split_once('=')) {
        let parsed = value.trim().parse().ok();
        match key {
            "memory.peak" => metrics.memory_peak = parsed,
            "pids.current" => metrics.pids_current = parsed,
            "pids.peak" => metrics.pids_peak = parsed,
            "pids.max" if value.trim() != "max" => metrics.pids_limit = parsed,
            _ => {}
        }
    }
    metrics
}

fn parse_df_bytes(output: &str) -> (Option<u64>, Option<u64>) {
    let fields = output
        .lines()
        .nth(1)
        .into_iter()
        .flat_map(str::split_whitespace)
        .collect::<Vec<_>>();
    (
        fields
            .get(1)
            .and_then(|value| value.parse::<u64>().ok())
            .map(|value| value.saturating_mul(1024)),
        fields
            .get(2)
            .and_then(|value| value.parse::<u64>().ok())
            .map(|value| value.saturating_mul(1024)),
    )
}

fn positive_u64(value: &serde_json::Value) -> Option<u64> {
    value.as_u64().filter(|value| *value > 0)
}

fn string_value(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn string_pairs(value: &serde_json::Value) -> Vec<(String, String)> {
    let mut pairs = value
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(key, _)| matches!(key.as_str(), "max-size" | "max-file"))
        .filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_string())))
        .collect::<Vec<_>>();
    pairs.sort();
    pairs
}

#[cfg(test)]
mod tests {
    use super::{parse_cgroup_metrics, parse_df_bytes};

    #[test]
    fn parses_available_cgroup_v2_metrics() {
        let metrics = parse_cgroup_metrics(
            "memory.peak=9296547840\npids.current=1015\npids.peak=1293\npids.max=4096\n",
        );

        assert_eq!(metrics.memory_peak, Some(9_296_547_840));
        assert_eq!(metrics.pids_current, Some(1015));
        assert_eq!(metrics.pids_peak, Some(1293));
        assert_eq!(metrics.pids_limit, Some(4096));
    }

    #[test]
    fn treats_unlimited_pid_cgroup_as_missing_limit() {
        let metrics = parse_cgroup_metrics("pids.current=8\npids.max=max\n");
        assert_eq!(metrics.pids_current, Some(8));
        assert_eq!(metrics.pids_limit, None);
    }

    #[test]
    fn parses_tmp_filesystem_bytes() {
        assert_eq!(
            parse_df_bytes(
                "Filesystem 1024-blocks Used Available Capacity Mounted on\n\
                 tmpfs 4194304 1024 4193280 1% /tmp\n"
            ),
            (Some(4_294_967_296), Some(1_048_576))
        );
    }
}
