use std::path::PathBuf;

use vm_packages::{SubmissionRecord, ToolKind, ToolSourceManifest};

use super::{git_output, managed_component, run, source_key, SourceManager};
use crate::{Store, WorkError, WorkResult};

impl SourceManager {
    /// Materialize one manifest-declared, catalog-registered source as an
    /// immutable Git bundle for the credential-free build service.
    pub(crate) async fn tool_build_source_bundle(
        &self,
        store: &Store,
        submission: &SubmissionRecord,
        requested_name: &str,
    ) -> WorkResult<PathBuf> {
        let requested_name = managed_component("build source name", requested_name)?;
        let integration = submission
            .integration
            .as_ref()
            .ok_or_else(|| WorkError::Conflict("integration is not prepared".into()))?;
        let integration_bundle = self.integration_bundle(submission)?;
        let inspect_root = self.root.join("build-source-inspection");
        tokio::fs::create_dir_all(&inspect_root).await?;
        let inspect = inspect_root.join(vm_core::secrets::generate_random_password(16));
        let inspect_result = async {
            run(
                self.git()
                    .arg("clone")
                    .arg("--bare")
                    .arg(&integration_bundle)
                    .arg(&inspect),
                "inspect binary tool build sources",
            )
            .await?;
            let raw = git_output(
                self.git()
                    .arg("--git-dir")
                    .arg(&inspect)
                    .arg("show")
                    .arg(format!("{}:vm-tool.yaml", integration.integration_commit)),
                "read validated binary tool manifest",
            )
            .await?;
            let manifest: ToolSourceManifest = serde_yaml_ng::from_str(&raw)
                .map_err(|error| WorkError::Invalid(format!("invalid vm-tool.yaml: {error}")))?;
            manifest.validate()?;
            if manifest.kind != ToolKind::Binary {
                return Err(WorkError::Invalid(
                    "build source request requires a binary tool manifest".into(),
                ));
            }
            manifest
                .build_sources
                .into_iter()
                .find(|source| source.name == requested_name)
                .ok_or_else(|| {
                    WorkError::Unauthorized(format!(
                        "build source {requested_name} is not declared by the validated manifest"
                    ))
                })
        }
        .await;
        let _ = tokio::fs::remove_dir_all(&inspect).await;
        let build_source = inspect_result?;

        let producer = store.tool(&submission.package).await?.definition;
        if !producer.build_sources.contains(&build_source.name) {
            return Err(WorkError::Unauthorized(format!(
                "build source {} is not authorized by the registered tool definition",
                build_source.name
            )));
        }

        let definition = store.source(&build_source.name).await?;
        if !matches!(
            definition.kind,
            vm_packages::SourceKind::ToolBinary | vm_packages::SourceKind::ToolCollection
        ) {
            return Err(WorkError::Invalid(format!(
                "build source {} is not a registered tool",
                build_source.name
            )));
        }
        let lock = self.lock(&format!("source:{}", definition.name)).await;
        let _guard = lock.lock().await;
        let mirror = self
            .root
            .join("sources")
            .join(format!("{}.git", source_key(&definition.name)));
        tokio::fs::create_dir_all(mirror.parent().expect("managed source mirror has a parent"))
            .await?;
        self.sync_mirror(&mirror, &definition.repository).await?;
        run(
            self.git()
                .arg("--git-dir")
                .arg(&mirror)
                .arg("cat-file")
                .arg("-e")
                .arg(format!("{}^{{commit}}", build_source.commit)),
            "verify immutable binary tool build source",
        )
        .await?;

        let directory = self
            .root
            .join("build-sources")
            .join(source_key(&definition.name));
        tokio::fs::create_dir_all(&directory).await?;
        let destination = directory.join(format!("{}.bundle", build_source.commit));
        if tokio::fs::try_exists(&destination).await? {
            return Ok(destination);
        }
        let reference = format!("refs/heads/vm-build-input-{}", build_source.commit);
        run(
            self.git()
                .arg("--git-dir")
                .arg(&mirror)
                .arg("update-ref")
                .arg(&reference)
                .arg(&build_source.commit),
            "retain immutable binary tool build source",
        )
        .await?;
        let temporary = directory.join(format!(
            ".{}.{}",
            build_source.commit,
            vm_core::secrets::generate_random_password(12)
        ));
        let bundle_result = run(
            self.git()
                .arg("--git-dir")
                .arg(&mirror)
                .arg("bundle")
                .arg("create")
                .arg(&temporary)
                .arg(&reference),
            "bundle immutable binary tool build source",
        )
        .await;
        if let Err(error) = bundle_result {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(error);
        }
        if let Err(error) = tokio::fs::rename(&temporary, &destination).await {
            let _ = tokio::fs::remove_file(&temporary).await;
            if !tokio::fs::try_exists(&destination).await? {
                return Err(error.into());
            }
        }
        Ok(destination)
    }
}
