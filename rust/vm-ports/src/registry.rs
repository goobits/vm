//! Port registry for tracking project port allocations.
//!
//! This module provides functionality for registering and managing port ranges
//! allocated to different projects, enabling conflict detection and suggesting
//! available port ranges.

// Standard library
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::path::PathBuf;

// External crates
use anyhow::Result;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use vm_common::vm_println;

// Internal imports
use crate::range::PortRange;

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
        let home_dir = std::env::var("HOME")
            .map_err(|_| anyhow::anyhow!("HOME environment variable not set"))?;
        let registry_dir = PathBuf::from(home_dir).join(".vm");
        let registry_path = registry_dir.join("port-registry.json");

        // Create registry directory if it doesn't exist
        if !registry_dir.exists() {
            fs::create_dir_all(&registry_dir)?;
        }

        // Initialize empty registry file if it doesn't exist
        if !registry_path.exists() {
            fs::write(&registry_path, "{}")?;
        }

        // Load registry from file with file locking
        let file = File::open(&registry_path)?;
        file.lock_shared()?; // Acquire shared lock for reading

        let content = fs::read_to_string(&registry_path)?;
        let entries: HashMap<String, ProjectEntry> =
            if content.trim().is_empty() || content.trim() == "{}" {
                HashMap::new()
            } else {
                serde_json::from_str(&content)?
            };

        file.unlock()?; // Release the lock

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

    /// Lists all registered project port ranges to stdout.
    pub fn list(&self) {
        if self.entries.is_empty() {
            vm_println!("No port ranges registered yet");
        } else {
            vm_println!("Registered port ranges:");
            vm_println!();

            // Sort entries by project name for consistent output
            let mut sorted_entries: Vec<_> = self.entries.iter().collect();
            sorted_entries.sort_by_key(|(name, _)| *name);

            for (project_name, entry) in sorted_entries {
                vm_println!("  {}: {} → {}", project_name, entry.range, entry.path);
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
        let mut current = start_from;

        while current + size - 1 < 65535 {
            let candidate_range = PortRange::new(current, current + size - 1).ok()?;

            // Check if this range conflicts
            if self.check_conflicts(&candidate_range, None).is_none() {
                return Some(candidate_range.to_string());
            }

            // Try next range
            current += size;
        }

        None
    }

    /// Performs an atomic update operation with exclusive file locking.
    /// This ensures thread-safe read-modify-write operations.
    fn atomic_update<F>(&mut self, update_fn: F) -> Result<()>
    where
        F: FnOnce(&mut HashMap<String, ProjectEntry>) -> Result<()>,
    {
        // Create or open the file with read/write permissions
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&self.registry_path)?;

        // Acquire exclusive lock for the entire read-modify-write operation
        file.lock_exclusive()?;

        // Read current state
        let content = fs::read_to_string(&self.registry_path)?;
        let mut entries: HashMap<String, ProjectEntry> =
            if content.trim().is_empty() || content.trim() == "{}" {
                HashMap::new()
            } else {
                serde_json::from_str(&content)?
            };

        // Apply the update
        update_fn(&mut entries)?;

        // Write back to file
        let json_content = if entries.is_empty() {
            String::from("{}")
        } else {
            serde_json::to_string_pretty(&entries)?
        };

        // Truncate and write directly (no temp file needed since we hold exclusive lock)
        file.set_len(0)?;
        fs::write(&self.registry_path, json_content)?;

        // Update our local state
        self.entries = entries;

        // Release the lock
        file.unlock()?;

        Ok(())
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
    fn test_concurrent_registry_access_with_file_locking() {
        use std::sync::Arc;
        use std::thread;

        let temp_dir =
            tempdir().expect("Failed to create temporary directory for file locking test");
        let registry_path = temp_dir.path().join("port-registry.json");

        // Initialize an empty registry file
        std::fs::write(&registry_path, "{}").expect("Failed to initialize empty registry file");

        // Create multiple registries that point to the same file (simulating different processes)
        let shared_path = Arc::new(registry_path);
        let mut handles = vec![];
        let num_threads = 10_usize;

        for i in 0..num_threads {
            let path = Arc::clone(&shared_path);
            let handle = thread::spawn(move || {
                // Each thread creates its own registry instance pointing to the same file
                let mut registry = PortRegistry {
                    entries: HashMap::new(),
                    registry_path: (*path).clone(),
                };

                // Add our entry using the fixed register method (which includes file locking)
                let range = PortRange::new(3000 + (i as u16) * 10, 3000 + (i as u16) * 10 + 9)
                    .expect("Valid range for file locking test");

                // Small delay to increase concurrency
                std::thread::sleep(std::time::Duration::from_millis(1));

                registry.register(&format!("project_{}", i), &range, &format!("/path_{}", i))
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        let results: Vec<_> = handles
            .into_iter()
            .map(|h| h.join().expect("Thread should complete successfully"))
            .collect();

        // Count successful vs failed operations
        let successful_registrations = results.iter().filter(|r| r.is_ok()).count();
        let failed_registrations = results.iter().filter(|r| r.is_err()).count();

        println!(
            "File locking test results: {} succeeded, {} failed",
            successful_registrations, failed_registrations
        );

        // Load final registry and check if all entries are present
        let content =
            std::fs::read_to_string(&*shared_path).expect("Failed to read final registry state");
        let final_entries: HashMap<String, ProjectEntry> =
            serde_json::from_str(&content).expect("Failed to parse final registry JSON");

        let actual_count = final_entries.len();

        println!("Final analysis with file locking:");
        println!("  File operations succeeded: {}", successful_registrations);
        println!("  File operations failed: {}", failed_registrations);
        println!("  Final registry entries: {}", actual_count);

        // With proper file locking, we should have all registrations succeed and be preserved
        if successful_registrations == actual_count && successful_registrations == num_threads {
            println!("✅ File locking successfully prevented race conditions!");
            println!("   All {} registrations were preserved", num_threads);
        } else {
            println!("⚠️  Unexpected behavior with file locking:");
            println!("   Expected: {} total registrations", num_threads);
            println!("   Achieved: {} stored registrations", actual_count);

            // This would indicate a problem with our file locking implementation
            if successful_registrations > actual_count {
                println!("🚨 File locking may not be working correctly!");
                println!(
                    "  {} operations succeeded, but only {} entries in final registry",
                    successful_registrations, actual_count
                );
            }
        }

        // Verify that all surviving entries are valid
        for (project_name, entry) in &final_entries {
            assert!(
                PortRange::parse(&entry.range).is_ok(),
                "Invalid range stored for project {}: {}",
                project_name,
                entry.range
            );
            assert!(
                entry.path.starts_with("/path_"),
                "Invalid path stored for project {}: {}",
                project_name,
                entry.path
            );
        }

        // Assert that file locking worked - all threads should succeed and all entries preserved
        assert_eq!(
            successful_registrations, num_threads,
            "File locking should prevent registration failures"
        );
        assert_eq!(
            actual_count, num_threads,
            "File locking should preserve all registrations"
        );
    }
}
