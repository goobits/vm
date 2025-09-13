use crate::range::PortRange;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub range: String,
    pub path: String,
}

#[derive(Debug, Default)]
pub struct PortRegistry {
    entries: HashMap<String, ProjectEntry>,
    registry_path: PathBuf,
}

impl PortRegistry {
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

        // Load registry from file
        let content = fs::read_to_string(&registry_path)?;
        let entries: HashMap<String, ProjectEntry> = if content.trim().is_empty() || content.trim() == "{}" {
            HashMap::new()
        } else {
            serde_json::from_str(&content)?
        };

        Ok(PortRegistry {
            entries,
            registry_path,
        })
    }

    pub fn check_conflicts(&self, range: &PortRange, exclude_project: Option<&str>) -> Option<String> {
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

    pub fn register(&mut self, project: &str, range: &PortRange, path: &str) -> Result<()> {
        let entry = ProjectEntry {
            range: range.to_string(),
            path: path.to_string(),
        };

        self.entries.insert(project.to_string(), entry);
        self.save()
    }

    pub fn list(&self) {
        if self.entries.is_empty() {
            println!("📡 No port ranges registered yet");
        } else {
            println!("📡 Registered port ranges:");
            println!();

            // Sort entries by project name for consistent output
            let mut sorted_entries: Vec<_> = self.entries.iter().collect();
            sorted_entries.sort_by_key(|(name, _)| *name);

            for (project_name, entry) in sorted_entries {
                println!("  {}: {} → {}", project_name, entry.range, entry.path);
            }
        }
    }

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

    fn save(&self) -> Result<()> {
        // Write to temporary file first for atomic operation
        let temp_path = self.registry_path.with_extension("tmp");

        let json_content = if self.entries.is_empty() {
            "{}".to_string()
        } else {
            serde_json::to_string_pretty(&self.entries)?
        };

        fs::write(&temp_path, json_content)?;

        // Atomic rename
        fs::rename(temp_path, &self.registry_path)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_conflict_detection() {
        let mut registry = PortRegistry {
            entries: HashMap::new(),
            registry_path: PathBuf::new(),
        };

        // Add a project
        let range1 = PortRange::new(3000, 3009).unwrap();
        registry.register("project1", &range1, "/path1").unwrap();

        // Test overlapping range
        let range2 = PortRange::new(3005, 3015).unwrap();
        let conflicts = registry.check_conflicts(&range2, None);
        assert!(conflicts.is_some());
        assert!(conflicts.unwrap().contains("project1"));

        // Test non-overlapping range
        let range3 = PortRange::new(3020, 3029).unwrap();
        let conflicts = registry.check_conflicts(&range3, None);
        assert!(conflicts.is_none());

        // Test excluding self from conflict check
        let conflicts = registry.check_conflicts(&range1, Some("project1"));
        assert!(conflicts.is_none());
    }

    #[test]
    fn test_suggest_next_range() {
        let mut registry = PortRegistry {
            entries: HashMap::new(),
            registry_path: PathBuf::new(),
        };

        // Register a range
        let range1 = PortRange::new(3000, 3009).unwrap();
        registry.register("project1", &range1, "/path1").unwrap();

        // Suggest next range
        let suggestion = registry.suggest_next_range(10, 3000);
        assert!(suggestion.is_some());
        let suggested = suggestion.unwrap();
        assert_eq!(suggested, "3010-3019"); // Should suggest non-overlapping range
    }
}