use anyhow::{Context, Result};
use std::path::PathBuf;
use vm_provider::MountPermission;

/// Mount parsing utilities
pub struct MountParser;

impl MountParser {
    /// Parse mount string in format "source:permissions" or "source:target:permissions"
    pub fn parse_mount_string(
        mount_str: &str,
    ) -> Result<(PathBuf, Option<PathBuf>, MountPermission)> {
        let parts: Vec<&str> = mount_str.split(':').collect();

        match parts.len() {
            1 => {
                // Just source path, use default permissions
                let source = PathBuf::from(parts[0]);
                Ok((source, None, MountPermission::default()))
            }
            2 => {
                // source:permissions
                let source = PathBuf::from(parts[0]);
                let permissions = parts[1].parse::<MountPermission>()
                    .with_context(|| format!("Invalid permission in mount string: {}", mount_str))?;
                Ok((source, None, permissions))
            }
            3 => {
                // source:target:permissions
                let source = PathBuf::from(parts[0]);
                let target = PathBuf::from(parts[1]);
                let permissions = parts[2].parse::<MountPermission>()
                    .with_context(|| format!("Invalid permission in mount string: {}", mount_str))?;
                Ok((source, Some(target), permissions))
            }
            _ => Err(anyhow::anyhow!(
                "Invalid mount string format: {}. Expected 'source', 'source:permissions', or 'source:target:permissions'",
                mount_str
            )),
        }
    }

    /// Parse multiple mount strings
    pub fn parse_mount_strings(
        mount_strings: &[String],
    ) -> Result<Vec<(PathBuf, Option<PathBuf>, MountPermission)>> {
        mount_strings
            .iter()
            .map(|s| Self::parse_mount_string(s))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mount_parser() {
        // Test simple source
        let (source, target, perm) = MountParser::parse_mount_string("/home/user")
            .expect("Should parse simple mount string");
        assert_eq!(source, PathBuf::from("/home/user"));
        assert_eq!(target, None);
        assert_eq!(perm, MountPermission::ReadWrite);

        // Test source with permissions
        let (source, target, perm) = MountParser::parse_mount_string("/home/user:ro")
            .expect("Should parse mount string with permissions");
        assert_eq!(source, PathBuf::from("/home/user"));
        assert_eq!(target, None);
        assert_eq!(perm, MountPermission::ReadOnly);

        // Test source with target and permissions
        let (source, target, perm) =
            MountParser::parse_mount_string("/home/user:/workspace/user:rw")
                .expect("Should parse mount string with target and permissions");
        assert_eq!(source, PathBuf::from("/home/user"));
        assert_eq!(target, Some(PathBuf::from("/workspace/user")));
        assert_eq!(perm, MountPermission::ReadWrite);
    }
}
