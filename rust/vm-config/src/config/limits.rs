use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::limit_parser::{LimitVisitor, ParsedLimit};

/// Memory limit configuration supporting fixed, percentage, and unlimited values.
#[derive(Debug, Clone, PartialEq)]
pub enum MemoryLimit {
    Limited(u32),
    Percentage(u8),
    Unlimited,
}

impl Serialize for MemoryLimit {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Limited(value) => serializer.serialize_u32(*value),
            Self::Percentage(value) => serializer.serialize_str(&format!("{value}%")),
            Self::Unlimited => serializer.serialize_str("unlimited"),
        }
    }
}

impl<'de> Deserialize<'de> for MemoryLimit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserializer.deserialize_any(LimitVisitor::new("memory (MB)"))? {
            ParsedLimit::Number(value) => Ok(Self::Limited(value)),
            ParsedLimit::Bytes(value) => Ok(Self::Limited((value / 1024 / 1024) as u32)),
            ParsedLimit::Percentage(value) => Ok(Self::Percentage(value)),
            ParsedLimit::Unlimited => Ok(Self::Unlimited),
        }
    }
}

impl MemoryLimit {
    pub fn to_mb(&self) -> Option<u32> {
        match self {
            Self::Limited(value) => Some(*value),
            Self::Percentage(_) | Self::Unlimited => None,
        }
    }

    pub fn is_unlimited(&self) -> bool {
        matches!(self, Self::Unlimited)
    }

    pub fn is_percentage(&self) -> bool {
        matches!(self, Self::Percentage(_))
    }

    pub fn to_percentage(&self) -> Option<u8> {
        match self {
            Self::Percentage(value) => Some(*value),
            _ => None,
        }
    }

    pub fn resolve_percentage(&self, available_mb: u64) -> Option<u32> {
        match self {
            Self::Limited(value) => Some(*value),
            Self::Percentage(value) => Some((available_mb * u64::from(*value) / 100) as u32),
            Self::Unlimited => None,
        }
    }

    pub fn to_docker_format(&self) -> Option<String> {
        self.to_mb().map(|value| format!("{value}m"))
    }
}

/// CPU limit configuration supporting fixed, percentage, and unlimited values.
#[derive(Debug, Clone, PartialEq)]
pub enum CpuLimit {
    Limited(u32),
    Percentage(u8),
    Unlimited,
}

impl Serialize for CpuLimit {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Limited(value) => serializer.serialize_u32(*value),
            Self::Percentage(value) => serializer.serialize_str(&format!("{value}%")),
            Self::Unlimited => serializer.serialize_str("unlimited"),
        }
    }
}

impl<'de> Deserialize<'de> for CpuLimit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        match deserializer.deserialize_any(LimitVisitor::new("CPU count"))? {
            ParsedLimit::Number(value) => Ok(Self::Limited(value)),
            ParsedLimit::Bytes(_) => Err(D::Error::custom(
                "Memory units (gb, mb) are not valid for CPU limits",
            )),
            ParsedLimit::Percentage(value) => Ok(Self::Percentage(value)),
            ParsedLimit::Unlimited => Ok(Self::Unlimited),
        }
    }
}

impl CpuLimit {
    pub fn to_count(&self) -> Option<u32> {
        match self {
            Self::Limited(value) => Some(*value),
            Self::Percentage(_) | Self::Unlimited => None,
        }
    }

    pub fn is_unlimited(&self) -> bool {
        matches!(self, Self::Unlimited)
    }

    pub fn is_percentage(&self) -> bool {
        matches!(self, Self::Percentage(_))
    }

    pub fn to_percentage(&self) -> Option<u8> {
        match self {
            Self::Percentage(value) => Some(*value),
            _ => None,
        }
    }

    pub fn resolve_percentage(&self, available_cpus: u32) -> Option<u32> {
        match self {
            Self::Limited(value) => Some(*value),
            Self::Percentage(value) => Some((available_cpus * u32::from(*value) / 100).max(1)),
            Self::Unlimited => None,
        }
    }
}

/// Swap limit configuration supporting fixed, percentage, and unlimited values.
#[derive(Debug, Clone, PartialEq)]
pub enum SwapLimit {
    Limited(u32),
    Percentage(u8),
    Unlimited,
}

impl Serialize for SwapLimit {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Limited(value) => serializer.serialize_u32(*value),
            Self::Percentage(value) => serializer.serialize_str(&format!("{value}%")),
            Self::Unlimited => serializer.serialize_str("unlimited"),
        }
    }
}

impl<'de> Deserialize<'de> for SwapLimit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserializer.deserialize_any(LimitVisitor::new("swap (MB)"))? {
            ParsedLimit::Number(value) => Ok(Self::Limited(value)),
            ParsedLimit::Bytes(value) => Ok(Self::Limited((value / 1024 / 1024) as u32)),
            ParsedLimit::Percentage(value) => Ok(Self::Percentage(value)),
            ParsedLimit::Unlimited => Ok(Self::Unlimited),
        }
    }
}

impl SwapLimit {
    pub fn to_mb(&self) -> Option<u32> {
        match self {
            Self::Limited(value) => Some(*value),
            Self::Percentage(_) | Self::Unlimited => None,
        }
    }

    pub fn is_unlimited(&self) -> bool {
        matches!(self, Self::Unlimited)
    }

    pub fn is_percentage(&self) -> bool {
        matches!(self, Self::Percentage(_))
    }

    pub fn to_percentage(&self) -> Option<u8> {
        match self {
            Self::Percentage(value) => Some(*value),
            _ => None,
        }
    }

    pub fn resolve_percentage(&self, available_mb: u64) -> Option<u32> {
        match self {
            Self::Limited(value) => Some(*value),
            Self::Percentage(value) => Some((available_mb * u64::from(*value) / 100) as u32),
            Self::Unlimited => None,
        }
    }
}

/// Disk limit configuration supporting fixed and percentage values.
#[derive(Debug, Clone, PartialEq)]
pub enum DiskLimit {
    Limited(u32),
    Percentage(u8),
}

impl Serialize for DiskLimit {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Limited(value) => serializer.serialize_u32(*value),
            Self::Percentage(value) => serializer.serialize_str(&format!("{value}%")),
        }
    }
}

impl<'de> Deserialize<'de> for DiskLimit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        match deserializer.deserialize_any(LimitVisitor::new("disk size (GB)"))? {
            ParsedLimit::Number(value) => Ok(Self::Limited(value)),
            ParsedLimit::Bytes(value) => Ok(Self::Limited((value / 1024 / 1024 / 1024) as u32)),
            ParsedLimit::Percentage(value) => Ok(Self::Percentage(value)),
            ParsedLimit::Unlimited => Err(D::Error::custom("Disk size cannot be unlimited")),
        }
    }
}

impl DiskLimit {
    pub fn to_gb(&self) -> Option<u32> {
        match self {
            Self::Limited(value) => Some(*value),
            Self::Percentage(_) => None,
        }
    }

    pub fn is_percentage(&self) -> bool {
        matches!(self, Self::Percentage(_))
    }

    pub fn to_percentage(&self) -> Option<u8> {
        match self {
            Self::Percentage(value) => Some(*value),
            _ => None,
        }
    }

    pub fn resolve_percentage(&self, available_gb: u64) -> Option<u32> {
        match self {
            Self::Limited(value) => Some(*value),
            Self::Percentage(value) => Some((available_gb * u64::from(*value) / 100) as u32),
        }
    }
}
