use anyhow::Result;

use crate::{
    CompleteToolBuildRequest, PublishToolArtifact, RegisterTool, SubmissionRecord,
    ToolArtifactRecord, ToolBuildRecord, ToolDefinition, ToolIndex, ToolInventory,
    ToolPublicationReceipt,
};

use super::PackageInfrastructureClient;

impl PackageInfrastructureClient {
    pub async fn register_tool(&self, request: &RegisterTool) -> Result<ToolDefinition> {
        self.post_work("v1/tools", request).await
    }

    pub async fn tools(&self) -> Result<Vec<ToolDefinition>> {
        self.get_work("v1/tools").await
    }

    pub async fn tool(&self, name: &str) -> Result<ToolInventory> {
        let name = url::form_urlencoded::byte_serialize(name.as_bytes()).collect::<String>();
        self.get_work(&format!("v1/tools/{name}")).await
    }

    pub async fn resolve_tool(
        &self,
        name: &str,
        version: Option<&str>,
        target: &str,
    ) -> Result<ToolArtifactRecord> {
        let name = url::form_urlencoded::byte_serialize(name.as_bytes()).collect::<String>();
        let query = {
            let mut query = url::form_urlencoded::Serializer::new(String::new());
            query.append_pair("target", target);
            if let Some(version) = version {
                query.append_pair("version", version);
            }
            query.finish()
        };
        self.get_work(&format!("v1/tools/{name}/resolve?{query}"))
            .await
    }

    pub async fn tool_index(&self, target: &str) -> Result<ToolIndex> {
        let query = {
            let mut query = url::form_urlencoded::Serializer::new(String::new());
            query.append_pair("target", target);
            query.finish()
        };
        self.get_work(&format!("v1/tools/index?{query}")).await
    }

    pub async fn publish_tool_artifact(
        &self,
        name: &str,
        request: &PublishToolArtifact,
    ) -> Result<ToolArtifactRecord> {
        let name = url::form_urlencoded::byte_serialize(name.as_bytes()).collect::<String>();
        self.post_release(&format!("v1/tools/{name}/artifacts"), request)
            .await
    }

    pub async fn tool_receipt(&self, receipt_id: &str) -> Result<ToolPublicationReceipt> {
        self.get_work(&format!("v1/tool-receipts/{receipt_id}"))
            .await
    }

    pub fn tool_artifact_url(&self, artifact: &ToolArtifactRecord) -> String {
        format!("{}{}", self.endpoints.gateway(), artifact.artifact_path)
    }

    pub async fn next_tool_build(&self) -> Result<Option<SubmissionRecord>> {
        self.get_authenticated("v1/jobs/build/next", self.build_token.as_deref(), "build")
            .await
    }

    pub async fn tool_build(&self, submission_id: &str) -> Result<ToolBuildRecord> {
        self.get_work(&format!("v1/submissions/{submission_id}/build"))
            .await
    }

    pub async fn complete_tool_build(
        &self,
        submission_id: &str,
        request: &CompleteToolBuildRequest,
    ) -> Result<ToolBuildRecord> {
        self.post_authenticated(
            &format!("v1/submissions/{submission_id}/build"),
            request,
            self.build_token
                .as_deref()
                .or(self.controller_token.as_deref()),
            "build",
        )
        .await
    }

    pub fn build_bundle_url(&self, submission_id: &str) -> String {
        self.work_url(&format!("v1/submissions/{submission_id}/build-bundle"))
    }
}
