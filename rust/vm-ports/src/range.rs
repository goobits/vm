use anyhow::Result;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct PortRange {
    pub start: u16,
    pub end: u16,
}

impl PortRange {
    pub fn parse(range_str: &str) -> Result<Self> {
        // Validate format: START-END
        if !range_str.contains('-') {
            anyhow::bail!("❌ Invalid port range format: {}\n💡 Expected format: START-END (e.g., 3170-3179)", range_str);
        }

        let parts: Vec<&str> = range_str.split('-').collect();
        if parts.len() != 2 {
            anyhow::bail!("❌ Invalid port range format: {}\n💡 Expected format: START-END (e.g., 3170-3179)", range_str);
        }

        let start: u16 = parts[0].parse()
            .map_err(|_| anyhow::anyhow!("❌ Invalid start port: {}", parts[0]))?;
        let end: u16 = parts[1].parse()
            .map_err(|_| anyhow::anyhow!("❌ Invalid end port: {}", parts[1]))?;

        if start >= end {
            anyhow::bail!("❌ Invalid range: start ({}) must be less than end ({})", start, end);
        }

        Ok(PortRange { start, end })
    }

    pub fn new(start: u16, end: u16) -> Result<Self> {
        if start >= end {
            anyhow::bail!("❌ Invalid range: start ({}) must be less than end ({})", start, end);
        }
        Ok(PortRange { start, end })
    }

    pub fn overlaps_with(&self, other: &PortRange) -> bool {
        // Ranges overlap if one starts before the other ends
        self.start <= other.end && other.start <= self.end
    }

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
        let range = PortRange::parse("3000-3009").unwrap();
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
        let range1 = PortRange::new(3000, 3009).unwrap();
        let range2 = PortRange::new(3005, 3015).unwrap();
        let range3 = PortRange::new(3010, 3019).unwrap();

        assert!(range1.overlaps_with(&range2)); // 3000-3009 overlaps 3005-3015
        assert!(range2.overlaps_with(&range1)); // symmetric
        assert!(!range1.overlaps_with(&range3)); // 3000-3009 doesn't overlap 3010-3019
        assert!(!range3.overlaps_with(&range1)); // symmetric
    }

    #[test]
    fn test_size() {
        let range = PortRange::new(3000, 3009).unwrap();
        assert_eq!(range.size(), 10);
    }

    // Edge case tests for production overlap detection bugs
    #[test]
    fn test_adjacent_ranges_no_overlap() {
        // Adjacent ranges should NOT overlap - common production bug
        let range1 = PortRange::new(3000, 3009).unwrap();
        let range2 = PortRange::new(3010, 3019).unwrap();

        assert!(!range1.overlaps_with(&range2), "Adjacent ranges 3000-3009 and 3010-3019 should not overlap");
        assert!(!range2.overlaps_with(&range1), "Overlap detection should be symmetric");
    }

    #[test]
    fn test_single_port_overlap_detection() {
        // Ranges that share exactly one port should overlap
        let range1 = PortRange::new(3000, 3009).unwrap();
        let range2 = PortRange::new(3009, 3019).unwrap(); // Shares port 3009

        assert!(range1.overlaps_with(&range2), "Ranges sharing port 3009 should overlap");
        assert!(range2.overlaps_with(&range1), "Overlap detection should be symmetric");
    }

    #[test]
    fn test_boundary_edge_cases() {
        // Test exact boundary conditions that cause off-by-one errors
        let range1 = PortRange::new(3000, 3009).unwrap();

        // Adjacent (no overlap)
        let adjacent = PortRange::new(3010, 3019).unwrap();
        assert!(!range1.overlaps_with(&adjacent));

        // Touching (overlap by 1)
        let touching = PortRange::new(3009, 3019).unwrap();
        assert!(range1.overlaps_with(&touching));

        // Single port ranges are invalid (start must be < end)
        assert!(PortRange::new(3009, 3009).is_err(), "Single port ranges should be invalid");
        assert!(PortRange::new(3000, 3000).is_err(), "Single port ranges should be invalid");

        // Minimal valid ranges (2 ports)
        let inside_last = PortRange::new(3009, 3010).unwrap();
        assert!(range1.overlaps_with(&inside_last));

        let inside_first = PortRange::new(3000, 3001).unwrap();
        assert!(range1.overlaps_with(&inside_first));
    }

    #[test]
    fn test_integer_overflow_boundary() {
        // Test near u16::MAX to catch overflow bugs
        let max_range = PortRange::new(65534, 65535).unwrap();
        let adjacent_to_max = PortRange::new(65533, 65534).unwrap();

        assert!(max_range.overlaps_with(&adjacent_to_max), "Should detect overlap at port 65534");

        // Test that we handle the boundary correctly
        let before_max = PortRange::new(65532, 65533).unwrap();
        assert!(!max_range.overlaps_with(&before_max), "Should not overlap 65534-65535 vs 65532-65533");
    }

    #[test]
    fn test_production_port_scenarios() {
        // Common production port allocations that have caused conflicts
        let web_ports = PortRange::new(3000, 3009).unwrap();
        let api_ports = PortRange::new(3010, 3019).unwrap();
        let db_ports = PortRange::new(5432, 5442).unwrap();

        // These should not conflict
        assert!(!web_ports.overlaps_with(&api_ports));
        assert!(!web_ports.overlaps_with(&db_ports));
        assert!(!api_ports.overlaps_with(&db_ports));

        // But this should conflict
        let conflicting_web = PortRange::new(3005, 3015).unwrap(); // Spans web and api
        assert!(web_ports.overlaps_with(&conflicting_web));
        assert!(api_ports.overlaps_with(&conflicting_web));
    }
}