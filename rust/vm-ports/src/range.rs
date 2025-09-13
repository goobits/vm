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
}