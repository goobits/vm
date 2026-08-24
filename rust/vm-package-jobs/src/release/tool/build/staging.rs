use std::path::Path;

use anyhow::{bail, Result};
use vm_packages::{validate_managed_id, ToolBuildArtifact};

use super::super::{artifact::BuiltToolArtifact, publication::publication_request};

pub(super) fn stage(
    root: &Path,
    submission_id: &str,
    artifacts: &[BuiltToolArtifact],
) -> Result<Vec<ToolBuildArtifact>> {
    validate_managed_id("submission ID", submission_id)?;
    let directory = root.join(submission_id);
    std::fs::create_dir_all(&directory)?;
    artifacts
        .iter()
        .map(|artifact| {
            let digest = &artifact.request.artifact_digest;
            let destination = directory.join(format!("{digest}.tar.gz"));
            if destination.exists() {
                let existing = publication_request(&destination, artifact.manifest.clone())?;
                if existing.artifact_digest != *digest {
                    bail!("staged tool artifact digest changed");
                }
            } else {
                let temporary = directory.join(format!(".{digest}.tmp"));
                std::fs::copy(&artifact.archive, &temporary)?;
                let copied = publication_request(&temporary, artifact.manifest.clone())?;
                if copied.artifact_digest != *digest {
                    let _ = std::fs::remove_file(&temporary);
                    bail!("copied tool artifact digest changed");
                }
                match std::fs::rename(&temporary, &destination) {
                    Ok(()) => {}
                    Err(error) if destination.exists() => {
                        let _ = std::fs::remove_file(&temporary);
                        let existing =
                            publication_request(&destination, artifact.manifest.clone())?;
                        if existing.artifact_digest != *digest {
                            return Err(error.into());
                        }
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            Ok(ToolBuildArtifact {
                target: artifact.request.target.clone(),
                artifact_digest: digest.clone(),
                size_bytes: artifact.request.size_bytes,
                links: artifact.request.links.clone(),
            })
        })
        .collect()
}
