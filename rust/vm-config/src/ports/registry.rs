//! Port registry for tracking project port allocations.
//!
//! This module provides functionality for registering and managing port ranges
//! allocated to different projects, enabling conflict detection and suggesting
//! available port ranges.

// Standard library
use std::collections::HashMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

// External crates
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use vm_core::error::VmError;
use vm_core::msg;
use vm_core::{error::Result, user_paths, vm_println};
use vm_messages::messages::MESSAGES;

// Internal imports
use super::range::PortRange;

/// A registry entry for a project's port allocation.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub range: String,
    pub path: String,
}

/// Registry for managing port range allocations across projects.
///
/// The registry stores project port assignments and provides conflict detection
/// and port range suggestion capabilities.
#[derive(Debug, Default)]
pub struct PortRegistry {
    entries: HashMap<String, ProjectEntry>,
    registry_path: PathBuf,
}

impl PortRegistry {
    /// Loads the port registry from the default location (`~/.vm/port-registry.json`).
    ///
    /// # Returns
    /// A `Result` containing the loaded registry or an error if loading fails.
    pub fn load() -> Result<Self> {
        let registry_path = user_paths::port_registry_path()?;
        let registry_dir = registry_path
            .parent()
            .ok_or_else(|| vm_core::error::VmError::Config("Invalid registry path".to_string()))?;

        // Create registry directory if it doesn't exist
        if !registry_dir.exists() {
            fs::create_dir_all(registry_dir)?;
        }

        let entries = Self::read_entries(&registry_path)?;

        Ok(PortRegistry {
            entries,
            registry_path,
        })
    }

    /// Checks if a port range conflicts with any registered projects.
    ///
    /// # Arguments
    /// * `range` - The port range to check for conflicts
    /// * `exclude_project` - Optional project name to exclude from conflict checking
    ///
    /// # Returns
    /// `Some(String)` containing conflicting project names if conflicts exist, `None` otherwise.
    pub fn check_conflicts(
        &self,
        range: &PortRange,
        exclude_project: Option<&str>,
    ) -> Option<String> {
        let mut conflicts = Vec::new();

        for (project_name, entry) in &self.entries {
            // Skip checking against self
            if let Some(excluded) = exclude_project {
                if project_name == excluded {
                    continue;
                }
            }

            // Parse the stored range and check for overlap
            if let Ok(other_range) = PortRange::parse(&entry.range) {
                if range.overlaps_with(&other_range) {
                    conflicts.push(format!("{} ({})", project_name, entry.range));
                }
            }
        }

        if conflicts.is_empty() {
            None
        } else {
            Some(conflicts.join(", "))
        }
    }

    /// Registers a port range for a project.
    ///
    /// # Arguments
    /// * `project` - The project name
    /// * `range` - The port range to register
    /// * `path` - The project path
    ///
    /// # Returns
    /// A `Result` indicating success or failure of the registration.
    pub fn register(&mut self, project: &str, range: &PortRange, path: &str) -> Result<()> {
        // Perform atomic read-modify-write operation with exclusive lock
        self.atomic_update(|entries| {
            let entry = ProjectEntry {
                range: range.to_string(),
                path: path.to_string(),
            };
            entries.insert(project.to_string(), entry);
            Ok(())
        })
    }

    /// Atomically reuse or reserve the next available range for a project.
    pub fn reserve_next_range(
        &mut self,
        project: &str,
        size: u16,
        start_from: u16,
        path: &str,
    ) -> Result<PortRange> {
        self.atomic_update(|entries| {
            if let Some(existing) = entries.get(project) {
                return PortRange::parse(&existing.range);
            }

            let range =
                Self::suggest_next_range_in(entries, size, start_from).ok_or_else(|| {
                    VmError::Config(format!(
                        "No available port range of size {size} at or above {start_from}"
                    ))
                })?;
            entries.insert(
                project.to_string(),
                ProjectEntry {
                    range: range.to_string(),
                    path: path.to_string(),
                },
            );
            Ok(range)
        })
    }

    /// Atomically replace a project's allocation with the next distinct range.
    pub fn replace_with_next_range(
        &mut self,
        project: &str,
        previous: &PortRange,
        size: u16,
        start_from: u16,
        path: &str,
    ) -> Result<PortRange> {
        self.atomic_update(|entries| {
            if let Some(existing) = entries.get(project) {
                let existing_range = PortRange::parse(&existing.range)?;
                if existing_range != *previous {
                    return Ok(existing_range);
                }
            }

            let range =
                Self::suggest_next_range_avoiding(entries, size, start_from, Some(previous))
                    .ok_or_else(|| {
                        VmError::Config(format!(
                            "No available port range of size {size} at or above {start_from}"
                        ))
                    })?;
            entries.insert(
                project.to_string(),
                ProjectEntry {
                    range: range.to_string(),
                    path: path.to_string(),
                },
            );
            Ok(range)
        })
    }

    /// Unregisters a project's port range.
    ///
    /// # Arguments
    /// * `project` - The project name to unregister
    ///
    /// # Returns
    /// A `Result` indicating success or failure of the unregistration.
    pub fn unregister(&mut self, project: &str) -> Result<()> {
        // Perform atomic read-modify-write operation with exclusive lock
        self.atomic_update(|entries| {
            entries.remove(project);
            Ok(())
        })
    }

    /// Gets a project's registry entry if it exists.
    ///
    /// # Arguments
    /// * `project` - The project name to look up
    ///
    /// # Returns
    /// `Some(&ProjectEntry)` if the project is registered, `None` otherwise.
    pub fn get_entry(&self, project: &str) -> Option<&ProjectEntry> {
        self.entries.get(project)
    }

    /// Lists all registered project port ranges to stdout.
    pub fn list(&self) {
        if self.entries.is_empty() {
            vm_println!("{}", MESSAGES.service.ports_no_ranges);
        } else {
            vm_println!("{}", MESSAGES.service.ports_registered_ranges);
            vm_println!();

            // Sort entries by project name for consistent output
            let mut sorted_entries: Vec<_> = self.entries.iter().collect();
            sorted_entries.sort_by_key(|(name, _)| *name);

            for (project_name, entry) in sorted_entries {
                vm_println!(
                    "{}",
                    msg!(
                        MESSAGES.service.ports_range_entry,
                        project = project_name,
                        range = &entry.range,
                        path = &entry.path
                    )
                );
            }
        }
    }

    /// Suggests the next available port range of the specified size.
    ///
    /// # Arguments
    /// * `size` - The number of ports needed
    /// * `start_from` - The starting port to search from
    ///
    /// # Returns
    /// `Some(String)` containing the suggested range, or `None` if no range is available.
    pub fn suggest_next_range(&self, size: u16, start_from: u16) -> Option<String> {
        Self::suggest_next_range_in(&self.entries, size, start_from).map(|range| range.to_string())
    }

    fn suggest_next_range_in(
        entries: &HashMap<String, ProjectEntry>,
        size: u16,
        start_from: u16,
    ) -> Option<PortRange> {
        Self::suggest_next_range_avoiding(entries, size, start_from, None)
    }

    fn suggest_next_range_avoiding(
        entries: &HashMap<String, ProjectEntry>,
        size: u16,
        start_from: u16,
        excluded: Option<&PortRange>,
    ) -> Option<PortRange> {
        let step = size.checked_sub(1)?;
        let mut current = start_from;

        loop {
            let end = current.checked_add(step)?;
            let candidate = PortRange::new(current, end).ok()?;
            let conflicts = entries.values().any(|entry| {
                PortRange::parse(&entry.range)
                    .map(|other| candidate.overlaps_with(&other))
                    .unwrap_or(false)
            });
            if !conflicts && excluded.map_or(true, |range| !candidate.overlaps_with(range)) {
                return Some(candidate);
            }
            current = current.checked_add(size)?;
        }
    }

    /// Performs an atomic update operation with proper file locking.
    /// This prevents race conditions during concurrent access to the registry file.
    fn atomic_update<T, F>(&mut self, update_fn: F) -> Result<T>
    where
        F: FnOnce(&mut HashMap<String, ProjectEntry>) -> Result<T>,
    {
        // Ensure parent directory exists
        if let Some(parent) = self.registry_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| {
                    VmError::Filesystem(format!(
                        "Failed to create registry directory: {parent:?}: {e}"
                    ))
                })?;
            }
        }

        // Lock a stable sidecar inode. The registry itself is atomically
        // replaced, so locking that file would not serialize later writers.
        let lock_path = self.registry_path.with_extension("lock");
        let lock_file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| {
                VmError::Filesystem(format!(
                    "Failed to open port registry lock {:?}: {e}",
                    lock_path
                ))
            })?;

        // fs2 supplies the MSRV-compatible locking API. Dropping lock_file
        // releases the lock because explicit std unlock requires newer Rust.
        const RETRY_DELAY: Duration = Duration::from_millis(10);
        const LOCK_TIMEOUT: Duration = Duration::from_secs(30);

        let lock_start = Instant::now();
        let mut attempts = 0;

        loop {
            match lock_file.try_lock_exclusive() {
                Ok(()) => break,
                Err(e) => {
                    attempts += 1;
                    if lock_start.elapsed() > LOCK_TIMEOUT {
                        return Err(vm_core::error::VmError::Internal(format!(
                            "Timeout waiting for exclusive lock on registry file after {attempts} attempts: {e}"
                        )));
                    }
                    std::thread::sleep(RETRY_DELAY);
                }
            }
        }

        let mut entries = Self::read_entries(&self.registry_path)?;

        // Apply the update
        let result = update_fn(&mut entries)?;

        // Write back to file
        let json_content = if entries.is_empty() {
            String::from("{}")
        } else {
            serde_json::to_string_pretty(&entries).map_err(|e| {
                VmError::Serialization(format!("Failed to serialize registry to JSON: {e}"))
            })?
        };

        vm_core::file_system::atomic_write(&self.registry_path, json_content.as_bytes()).map_err(
            |e| {
                VmError::Filesystem(format!(
                    "Failed to atomically write port registry {:?}: {e}",
                    self.registry_path
                ))
            },
        )?;

        // Update our local state
        self.entries = entries;

        Ok(result)
    }

    fn read_entries(path: &Path) -> Result<HashMap<String, ProjectEntry>> {
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(HashMap::new()),
            Err(error) => return Err(error.into()),
        };
        if content.trim().is_empty() || content.trim() == "{}" {
            Ok(HashMap::new())
        } else {
            serde_json::from_str(&content).map_err(|error| {
                VmError::Serialization(format!("Failed to parse registry JSON: {error}"))
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_conflict_detection() {
        let temp_file = tempfile::NamedTempFile::new()
            .expect("Failed to create temporary file for conflict detection test");
        let mut registry = PortRegistry {
            entries: HashMap::new(),
            registry_path: temp_file.path().to_path_buf(),
        };

        // Add a project
        let range1 = PortRange::new(3000, 3009).expect("Valid range for conflict detection test");
        registry
            .register("project1", &range1, "/path1")
            .expect("Failed to register project1 for test");

        // Test overlapping range
        let range2 = PortRange::new(3005, 3015).expect("Valid overlapping range for conflict test");
        let conflicts = registry.check_conflicts(&range2, None);
        assert!(conflicts.is_some());
        assert!(conflicts
            .expect("Should have conflicts for overlapping range")
            .contains("project1"));

        // Test non-overlapping range
        let range3 =
            PortRange::new(3020, 3029).expect("Valid non-overlapping range for conflict test");
        let conflicts = registry.check_conflicts(&range3, None);
        assert!(conflicts.is_none());

        // Test excluding self from conflict check
        let conflicts = registry.check_conflicts(&range1, Some("project1"));
        assert!(conflicts.is_none());
    }

    #[test]
    fn test_suggest_next_range() {
        let temp_file = tempfile::NamedTempFile::new()
            .expect("Failed to create temporary file for suggestion test");
        let mut registry = PortRegistry {
            entries: HashMap::new(),
            registry_path: temp_file.path().to_path_buf(),
        };

        // Register a range
        let range1 = PortRange::new(3000, 3009).expect("Valid range for suggestion test");
        registry
            .register("project1", &range1, "/path1")
            .expect("Failed to register project1 for suggestion test");

        // Suggest next range
        let suggestion = registry.suggest_next_range(10, 3000);
        assert!(suggestion.is_some());
        let suggested = suggestion.expect("Should suggest a valid next range");
        assert_eq!(suggested, "3010-3019"); // Should suggest non-overlapping range
    }

    #[test]
    fn test_concurrent_registry_access_with_locking() {
        use std::sync::Arc;
        use std::thread;

        let temp_dir = tempdir().unwrap();
        let registry_path = Arc::new(temp_dir.path().join("port-registry.json"));
        let handles: Vec<_> = (0..10)
            .map(|index| {
                let path = Arc::clone(&registry_path);
                thread::spawn(move || {
                    let mut registry = PortRegistry {
                        entries: HashMap::new(),
                        registry_path: (*path).clone(),
                    };
                    let start = 3000 + index * 10;
                    let range = PortRange::new(start, start + 9).unwrap();
                    registry.register(
                        &format!("project_{index}"),
                        &range,
                        &format!("/path_{index}"),
                    )
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap().unwrap();
        }

        let entries = PortRegistry::read_entries(&registry_path).unwrap();
        assert_eq!(entries.len(), 10);
        for index in 0..10 {
            let entry = &entries[&format!("project_{index}")];
            assert_eq!(
                entry.range,
                format!("{}-{}", 3000 + index * 10, 3009 + index * 10)
            );
            assert_eq!(entry.path, format!("/path_{index}"));
        }
    }

    #[test]
    fn concurrent_reservations_are_preserved_and_unique() {
        use std::collections::HashSet;
        use std::sync::Arc;
        use std::thread;

        let temp_dir = tempdir().expect("create registry directory");
        let registry_path = Arc::new(temp_dir.path().join("port-registry.json"));
        let handles: Vec<_> = (0..10)
            .map(|index| {
                let path = Arc::clone(&registry_path);
                thread::spawn(move || {
                    let mut registry = PortRegistry {
                        entries: HashMap::new(),
                        registry_path: (*path).clone(),
                    };
                    registry.reserve_next_range(
                        &format!("project_{index}"),
                        10,
                        3000,
                        &format!("/path_{index}"),
                    )
                })
            })
            .collect();

        let ranges: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap().unwrap().to_string())
            .collect();
        assert_eq!(ranges.iter().collect::<HashSet<_>>().len(), ranges.len());

        let entries = PortRegistry::read_entries(&registry_path).unwrap();
        assert_eq!(entries.len(), ranges.len());
        assert!(ranges
            .iter()
            .all(|range| entries.values().any(|entry| &entry.range == range)));
    }

    #[test]
    fn replacement_reuses_a_reserved_range_after_a_config_write_retry() {
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        let mut registry = PortRegistry {
            entries: HashMap::new(),
            registry_path: temp_file.path().to_path_buf(),
        };
        let previous = PortRange::new(3000, 3009).unwrap();
        registry.register("project", &previous, "/project").unwrap();

        let replacement = registry
            .replace_with_next_range("project", &previous, 10, 3000, "/project")
            .unwrap();
        assert_eq!(replacement.to_string(), "3010-3019");

        let retried = registry
            .replace_with_next_range("project", &previous, 10, 3000, "/project")
            .unwrap();
        assert_eq!(retried, replacement);
    }
}
