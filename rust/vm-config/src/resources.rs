use serde::{Deserialize, Serialize};
use sysinfo::System;

pub fn detect_resource_defaults() -> ResourceSuggestion {
    let sys = System::new_all();

    let total_memory = sys.total_memory() / 1024; // KB to MB
    let total_cpus = sys.cpus().len() as u32;

    // Use 50% of system resources, with minimums
    ResourceSuggestion {
        memory: std::cmp::max(2048, total_memory as u32 / 2), // Min 2GB, max 50%
        cpus: std::cmp::max(2, total_cpus / 2),               // Min 2, max 50%
        disk_size: None,
    }
}

/// VM resource allocation suggestion based on project type.
///
/// Represents recommended hardware resource allocations for different types of
/// development projects. These suggestions are based on typical resource usage
/// patterns for various technology stacks and development workflows.
///
/// # Fields
/// - `memory`: RAM allocation in megabytes
/// - `cpus`: Number of CPU cores to allocate
/// - `disk_size`: Optional disk size in gigabytes (if different from default)
///
/// # Examples
/// ```rust
/// use vm_config::resources::ResourceSuggestion;
///
/// let suggestion = ResourceSuggestion {
///     memory: 2048,  // 2GB RAM
///     cpus: 2,       // 2 CPU cores
///     disk_size: Some(20), // 20GB disk
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceSuggestion {
    pub memory: u32,            // Memory in MB
    pub cpus: u32,              // Number of CPU cores
    pub disk_size: Option<u32>, // Disk size in GB (when different from default)
}

/// Resource advisor for VM configurations.
///
/// Provides intelligent resource allocation recommendations based on project types
/// and development requirements. The advisor considers typical resource usage
/// patterns for different technology stacks to suggest optimal VM configurations.
///
/// ## Recommendation Logic
/// Resource suggestions are based on:
/// - **Language/Framework Requirements**: Memory and CPU needs for different stacks
/// - **Development Workflows**: Build processes, hot reloading, testing
/// - **Typical Dependencies**: Database requirements, asset compilation
/// - **Performance Considerations**: Avoiding resource constraints during development
///
/// ## Supported Project Types
/// - Frontend frameworks (React, Vue, Angular)
/// - Full-stack frameworks (Next.js)
/// - Backend frameworks (Django, Flask, Rails)
/// - Language runtimes (Node.js, Python, Rust, Go)
/// - Infrastructure tools (Docker, Kubernetes)
pub struct ResourceAdvisor;

impl ResourceAdvisor {
    /// Suggest VM resources based on project type.
    ///
    /// Analyzes the project type and returns optimized resource allocation
    /// recommendations. The suggestions balance performance with resource
    /// efficiency, ensuring smooth development while avoiding over-allocation.
    ///
    /// ## Resource Tiers
    /// - **Light** (1-2GB, 1-2 CPUs): Simple projects, basic development
    /// - **Moderate** (2-3GB, 2 CPUs): Standard web development, most frameworks
    /// - **Heavy** (4-8GB, 2-4 CPUs): Complex builds, multiple services
    /// - **Intensive** (8GB+, 4+ CPUs): Large projects, extensive tooling
    ///
    /// # Arguments
    /// * `project_type` - Project type identifier (e.g., "react", "django", "rust")
    ///
    /// # Returns
    /// A `ResourceSuggestion` with recommended memory, CPU, and optional disk allocations
    ///
    /// # Examples
    /// ```rust
    /// use vm_config::resources::ResourceAdvisor;
    ///
    /// let suggestion = ResourceAdvisor::suggest_vm_resources("react");
    /// println!("Recommended: {}MB RAM, {} CPUs", suggestion.memory, suggestion.cpus);
    ///
    /// let heavy_suggestion = ResourceAdvisor::suggest_vm_resources("kubernetes");
    /// println!("For K8s: {}MB RAM, {} CPUs", heavy_suggestion.memory, heavy_suggestion.cpus);
    /// ```
    pub fn suggest_vm_resources(project_type: &str) -> ResourceSuggestion {
        match project_type {
            // Frontend frameworks - moderate resources
            "react" | "vue" | "angular" => ResourceSuggestion {
                memory: 2048,
                cpus: 2,
                disk_size: None,
            },

            // Next.js - slightly more resources due to SSR
            "next" => ResourceSuggestion {
                memory: 3072,
                cpus: 2,
                disk_size: None,
            },

            // Backend frameworks - more memory for database connections
            "django" | "flask" | "rails" => ResourceSuggestion {
                memory: 3072,
                cpus: 2,
                disk_size: None,
            },

            // Generic Node.js - moderate resources
            "nodejs" => ResourceSuggestion {
                memory: 2048,
                cpus: 2,
                disk_size: None,
            },

            // Python - moderate resources
            "python" => ResourceSuggestion {
                memory: 2048,
                cpus: 2,
                disk_size: None,
            },

            // Compiled languages - more CPU for building
            "rust" => ResourceSuggestion {
                memory: 4096,
                cpus: 4,
                disk_size: None,
            },

            "go" => ResourceSuggestion {
                memory: 2048,
                cpus: 4,
                disk_size: None,
            },

            // Container/orchestration - more resources for multiple services
            "docker" => ResourceSuggestion {
                memory: 4096,
                cpus: 2,
                disk_size: Some(40), // More disk for container images
            },

            "kubernetes" => ResourceSuggestion {
                memory: 6144,
                cpus: 4,
                disk_size: Some(60), // Even more disk for k8s images and data
            },

            // Multi-technology projects - parse and aggregate
            project if project.starts_with("multi:") => Self::suggest_multi_tech_resources(project),

            // Generic/unknown - conservative default
            _ => ResourceSuggestion {
                memory: 2048,
                cpus: 2,
                disk_size: None,
            },
        }
    }

    /// Handle multi-technology project resource calculation
    fn suggest_multi_tech_resources(project_type: &str) -> ResourceSuggestion {
        let tech_str = project_type.strip_prefix("multi:").unwrap_or("");
        let technologies: Vec<&str> = tech_str.split_whitespace().collect();

        if technologies.is_empty() {
            return Self::suggest_vm_resources("generic");
        }

        // Get suggestions for each technology
        let suggestions: Vec<ResourceSuggestion> = technologies
            .iter()
            .map(|tech| Self::suggest_vm_resources(tech))
            .collect();

        // Aggregate resources - take max of each dimension
        let max_memory = suggestions.iter().map(|s| s.memory).max().unwrap_or(2048);
        let max_cpus = suggestions.iter().map(|s| s.cpus).max().unwrap_or(2);
        let max_disk = suggestions.iter().filter_map(|s| s.disk_size).max();

        // Add 50% overhead for multi-tech complexity
        let memory_with_overhead = (max_memory as f32 * 1.5) as u32;
        let cpus_with_overhead = std::cmp::max(max_cpus + 1, 2);

        ResourceSuggestion {
            memory: memory_with_overhead,
            cpus: cpus_with_overhead,
            disk_size: max_disk,
        }
    }

    /// Format resource suggestion as shell-compatible string
    /// (for compatibility with existing shell tests)
    pub fn format_as_shell_output(suggestion: &ResourceSuggestion) -> String {
        let mut parts = vec![
            format!("memory={}", suggestion.memory),
            format!("cpus={}", suggestion.cpus),
        ];

        if let Some(disk) = suggestion.disk_size {
            parts.push(format!("disk_size={}", disk));
        }

        parts.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_react_resources() {
        let suggestion = ResourceAdvisor::suggest_vm_resources("react");
        assert_eq!(suggestion.memory, 2048);
        assert_eq!(suggestion.cpus, 2);
        assert_eq!(suggestion.disk_size, None);

        let shell_output = ResourceAdvisor::format_as_shell_output(&suggestion);
        assert!(shell_output.contains("memory=2048"));
        assert!(shell_output.contains("cpus=2"));
    }

    #[test]
    fn test_rust_resources() {
        let suggestion = ResourceAdvisor::suggest_vm_resources("rust");
        assert_eq!(suggestion.memory, 4096);
        assert_eq!(suggestion.cpus, 4);
        assert_eq!(suggestion.disk_size, None);

        let shell_output = ResourceAdvisor::format_as_shell_output(&suggestion);
        assert!(shell_output.contains("memory=4096"));
        assert!(shell_output.contains("cpus=4"));
    }

    #[test]
    fn test_docker_resources() {
        let suggestion = ResourceAdvisor::suggest_vm_resources("docker");
        assert_eq!(suggestion.memory, 4096);
        assert_eq!(suggestion.cpus, 2);
        assert_eq!(suggestion.disk_size, Some(40));

        let shell_output = ResourceAdvisor::format_as_shell_output(&suggestion);
        assert!(shell_output.contains("memory=4096"));
        assert!(shell_output.contains("disk_size=40"));
    }

    #[test]
    fn test_multi_tech_resources() {
        let suggestion = ResourceAdvisor::suggest_vm_resources("multi:react django");
        // Should take max memory (3072 from django) + 50% overhead = 4608
        assert_eq!(suggestion.memory, 4608);
        // Should take max cpus (2) + 1 overhead = 3
        assert_eq!(suggestion.cpus, 3);
        assert_eq!(suggestion.disk_size, None);
    }

    #[test]
    fn test_multi_tech_with_docker() {
        let suggestion = ResourceAdvisor::suggest_vm_resources("multi:react docker");
        // Should take max memory (4096 from docker) + 50% overhead = 6144
        assert_eq!(suggestion.memory, 6144);
        // Should take max cpus (2) + 1 overhead = 3
        assert_eq!(suggestion.cpus, 3);
        // Should preserve disk requirement from docker
        assert_eq!(suggestion.disk_size, Some(40));
    }

    #[test]
    fn test_generic_fallback() {
        let suggestion = ResourceAdvisor::suggest_vm_resources("unknown-framework");
        assert_eq!(suggestion.memory, 2048);
        assert_eq!(suggestion.cpus, 2);
        assert_eq!(suggestion.disk_size, None);
    }

    #[test]
    fn test_all_framework_types() {
        // Test that all frameworks mentioned in shell tests work
        let frameworks = vec![
            "react",
            "vue",
            "angular",
            "next",
            "django",
            "flask",
            "rails",
            "nodejs",
            "python",
            "rust",
            "go",
            "docker",
            "kubernetes",
        ];

        for framework in frameworks {
            let suggestion = ResourceAdvisor::suggest_vm_resources(framework);
            assert!(
                suggestion.memory >= 2048,
                "Framework {} should have at least 2GB memory",
                framework
            );
            assert!(
                suggestion.cpus >= 2,
                "Framework {} should have at least 2 CPUs",
                framework
            );
        }
    }

    #[test]
    fn test_shell_output_format_compatibility() {
        // Test formats match what shell tests expect
        let suggestion = ResourceSuggestion {
            memory: 2048,
            cpus: 2,
            disk_size: None,
        };
        let output = ResourceAdvisor::format_as_shell_output(&suggestion);
        assert_eq!(output, "memory=2048 cpus=2");

        let suggestion_with_disk = ResourceSuggestion {
            memory: 4096,
            cpus: 4,
            disk_size: Some(40),
        };
        let output_with_disk = ResourceAdvisor::format_as_shell_output(&suggestion_with_disk);
        assert_eq!(output_with_disk, "memory=4096 cpus=4 disk_size=40");
    }
}
