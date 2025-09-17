//! Port range management utilities.
//!
//! This module provides types and functions for representing and manipulating
//! network port ranges, including parsing, validation, and overlap detection.

use anyhow::Result;
use std::fmt;
use vm_common::vm_error;

#[cfg(test)]
mod test_constants {
    // These constants were defined for consistency but are unused in practice
    // Tests use inline values for clarity
    #[allow(dead_code)]
    pub const DEFAULT_START_PORT: u16 = 3000;
    #[allow(dead_code)]
    pub const DEFAULT_END_PORT: u16 = 3009;
    #[allow(dead_code)]
    pub const OVERLAP_START_PORT: u16 = 3005;
    #[allow(dead_code)]
    pub const OVERLAP_END_PORT: u16 = 3015;
    #[allow(dead_code)]
    pub const ADJACENT_START_PORT: u16 = 3010;
    #[allow(dead_code)]
    pub const ADJACENT_END_PORT: u16 = 3019;
}

/// Represents a range of network ports.
///
/// A port range defines a continuous range of ports from `start` to `end` (inclusive).
/// The start port must always be less than the end port.
#[derive(Debug, Clone, PartialEq)]
pub struct PortRange {
    pub start: u16,
    pub end: u16,
}

impl PortRange {
    /// Parses a port range from a string in "START-END" format.
    ///
    /// # Arguments
    /// * `range_str` - A string containing the port range (e.g., "3000-3009")
    ///
    /// # Returns
    /// A `Result` containing the parsed `PortRange` or an error if the format is invalid.
    pub fn parse(range_str: &str) -> Result<Self> {
        // Validate format: START-END
        if !range_str.contains('-') {
            vm_error!(
                "Invalid port range format: {}\n💡 Expected format: START-END (e.g., 3170-3179)",
                range_str
            );
            return Err(anyhow::anyhow!("Invalid port range format"));
        }

        let parts: Vec<&str> = range_str.split('-').collect();
        if parts.len() != 2 {
            vm_error!(
                "Invalid port range format: {}\n💡 Expected format: START-END (e.g., 3170-3179)",
                range_str
            );
            return Err(anyhow::anyhow!("Invalid port range format"));
        }

        let start: u16 = parts[0]
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid start port: {}", parts[0]))?;
        let end: u16 = parts[1]
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid end port: {}", parts[1]))?;

        if start >= end {
            vm_error!(
                "Invalid range: start ({}) must be less than end ({})",
                start,
                end
            );
            return Err(anyhow::anyhow!("Invalid range values"));
        }

        Ok(PortRange { start, end })
    }

    /// Creates a new port range with the specified start and end ports.
    ///
    /// # Arguments
    /// * `start` - The starting port number
    /// * `end` - The ending port number
    ///
    /// # Returns
    /// A `Result` containing the new `PortRange` or an error if start >= end.
    pub fn new(start: u16, end: u16) -> Result<Self> {
        if start >= end {
            vm_error!(
                "Invalid range: start ({}) must be less than end ({})",
                start,
                end
            );
            return Err(anyhow::anyhow!("Invalid range values"));
        }
        Ok(PortRange { start, end })
    }

    /// Checks if this port range overlaps with another port range.
    ///
    /// # Arguments
    /// * `other` - The other port range to check for overlap
    ///
    /// # Returns
    /// `true` if the ranges overlap, `false` otherwise.
    pub fn overlaps_with(&self, other: &PortRange) -> bool {
        // Ranges overlap if one starts before the other ends
        self.start <= other.end && other.start <= self.end
    }

    /// Returns the number of ports in this range.
    ///
    /// # Returns
    /// The count of ports in the range (end - start + 1).
    #[allow(dead_code)]
    pub fn size(&self) -> u16 {
        self.end - self.start + 1
    }
}

impl fmt::Display for PortRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.start, self.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_range() {
        let range = PortRange::parse("3000-3009").expect("Valid port range format for test");
        assert_eq!(range.start, 3000);
        assert_eq!(range.end, 3009);
    }

    #[test]
    fn test_parse_invalid_format() {
        assert!(PortRange::parse("3000").is_err());
        assert!(PortRange::parse("3000-3009-3010").is_err());
        assert!(PortRange::parse("invalid-range").is_err());
    }

    #[test]
    fn test_parse_invalid_range() {
        assert!(PortRange::parse("3009-3000").is_err()); // start >= end
        assert!(PortRange::parse("3000-3000").is_err()); // start == end
    }

    #[test]
    fn test_overlap_detection() {
        let range1 = PortRange::new(3000, 3009).expect("Valid first range for overlap test");
        let range2 = PortRange::new(3005, 3015).expect("Valid second range for overlap test");
        let range3 = PortRange::new(3010, 3019).expect("Valid third range for overlap test");

        assert!(range1.overlaps_with(&range2)); // 3000-3009 overlaps 3005-3015
        assert!(range2.overlaps_with(&range1)); // symmetric
        assert!(!range1.overlaps_with(&range3)); // 3000-3009 doesn't overlap 3010-3019
        assert!(!range3.overlaps_with(&range1)); // symmetric
    }

    #[test]
    fn test_size() {
        let range = PortRange::new(3000, 3009).expect("Valid range for size test");
        assert_eq!(range.size(), 10);
    }

    // Edge case tests for production overlap detection bugs
    #[test]
    fn test_adjacent_ranges_no_overlap() {
        // Adjacent ranges should NOT overlap - common production bug
        let range1 = PortRange::new(3000, 3009).expect("Valid first range for adjacent test");
        let range2 = PortRange::new(3010, 3019).expect("Valid second range for adjacent test");

        assert!(
            !range1.overlaps_with(&range2),
            "Adjacent ranges 3000-3009 and 3010-3019 should not overlap"
        );
        assert!(
            !range2.overlaps_with(&range1),
            "Overlap detection should be symmetric"
        );
    }

    #[test]
    fn test_single_port_overlap_detection() {
        // Ranges that share exactly one port should overlap
        let range1 =
            PortRange::new(3000, 3009).expect("Valid first range for single port overlap test");
        let range2 =
            PortRange::new(3009, 3019).expect("Valid second range for single port overlap test"); // Shares port 3009

        assert!(
            range1.overlaps_with(&range2),
            "Ranges sharing port 3009 should overlap"
        );
        assert!(
            range2.overlaps_with(&range1),
            "Overlap detection should be symmetric"
        );
    }

    #[test]
    fn test_boundary_edge_cases() {
        // Test exact boundary conditions that cause off-by-one errors
        let range1 = PortRange::new(3000, 3009).expect("Valid base range for boundary test");

        // Adjacent (no overlap)
        let adjacent = PortRange::new(3010, 3019).expect("Valid adjacent range for boundary test");
        assert!(!range1.overlaps_with(&adjacent));

        // Touching (overlap by 1)
        let touching = PortRange::new(3009, 3019).expect("Valid touching range for boundary test");
        assert!(range1.overlaps_with(&touching));

        // Single port ranges are invalid (start must be < end)
        assert!(
            PortRange::new(3009, 3009).is_err(),
            "Single port ranges should be invalid"
        );
        assert!(
            PortRange::new(3000, 3000).is_err(),
            "Single port ranges should be invalid"
        );

        // Minimal valid ranges (2 ports)
        let inside_last = PortRange::new(3009, 3010).expect("Valid minimal range at end boundary");
        assert!(range1.overlaps_with(&inside_last));

        let inside_first =
            PortRange::new(3000, 3001).expect("Valid minimal range at start boundary");
        assert!(range1.overlaps_with(&inside_first));
    }

    #[test]
    fn test_integer_overflow_boundary() {
        // Test near u16::MAX to catch overflow bugs
        let max_range = PortRange::new(65534, 65535).expect("Valid range at maximum port boundary");
        let adjacent_to_max =
            PortRange::new(65533, 65534).expect("Valid range adjacent to maximum");

        assert!(
            max_range.overlaps_with(&adjacent_to_max),
            "Should detect overlap at port 65534"
        );

        // Test that we handle the boundary correctly
        let before_max = PortRange::new(65532, 65533).expect("Valid range before maximum boundary");
        assert!(
            !max_range.overlaps_with(&before_max),
            "Should not overlap 65534-65535 vs 65532-65533"
        );
    }

    #[test]
    fn test_production_port_scenarios() {
        // Common production port allocations that have caused conflicts
        let web_ports =
            PortRange::new(3000, 3009).expect("Valid web ports range for production test");
        let api_ports =
            PortRange::new(3010, 3019).expect("Valid API ports range for production test");
        let db_ports =
            PortRange::new(5432, 5442).expect("Valid database ports range for production test");

        // These should not conflict
        assert!(!web_ports.overlaps_with(&api_ports));
        assert!(!web_ports.overlaps_with(&db_ports));
        assert!(!api_ports.overlaps_with(&db_ports));

        // But this should conflict
        let conflicting_web =
            PortRange::new(3005, 3015).expect("Valid conflicting range for production test"); // Spans web and api
        assert!(web_ports.overlaps_with(&conflicting_web));
        assert!(api_ports.overlaps_with(&conflicting_web));
    }
}
