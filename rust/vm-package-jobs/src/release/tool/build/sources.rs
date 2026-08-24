use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use vm_packages::{PackageInfrastructureClient, ToolSourceManifest};

use crate::release::{git_text, source::clone_at};
use crate::runtime::download_bundle;

pub(super) fn materialize(
    client: &PackageInfrastructureClient,
    submission_id: &str,
    build_token: &str,
    release_root: &Path,
    manifest: &ToolSourceManifest,
) -> Result<Vec<PathBuf>> {
    manifest
        .build_sources
        .iter()
        .map(|build_source| {
            let bundle = release_root.join(format!("{}.bundle", build_source.name));
            download_bundle(
                &client.tool_build_source_url(submission_id, &build_source.name),
                build_token,
                &bundle,
            )?;
            let destination = release_root.join(&build_source.name);
            clone_at(&bundle, &destination, &build_source.commit)?;
            let resolved = git_text(
                &destination,
                &["rev-parse", "--verify", "HEAD^{commit}"],
                "resolve immutable binary tool build source",
            )?;
            if resolved != build_source.commit {
                bail!(
                    "binary tool build source {} does not match its declared commit",
                    build_source.name
                );
            }
            Ok(destination)
        })
        .collect()
}
