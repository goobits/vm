use anyhow::Result;

use crate::{
    ClaimToolActivationRequest, FinishToolActivationRequest, PlanToolActivationRequest,
    ToolActivationRecord, UpdateToolActivationTargetRequest,
};

use super::PackageInfrastructureClient;

impl PackageInfrastructureClient {
    pub async fn tool_activation_for_release(
        &self,
        release_id: &str,
    ) -> Result<ToolActivationRecord> {
        self.get_work(&format!(
            "v1/releases/{}/tool-activation",
            encode(release_id)
        ))
        .await
    }

    pub async fn tool_activations(&self) -> Result<Vec<ToolActivationRecord>> {
        self.get_authenticated(
            "v1/tool-activations",
            self.controller_token.as_deref(),
            "controller",
        )
        .await
    }

    pub async fn claim_next_tool_activation(
        &self,
        request: &ClaimToolActivationRequest,
    ) -> Result<Option<ToolActivationRecord>> {
        self.post_work("v1/jobs/tool-activation/next", request)
            .await
    }

    pub async fn claim_tool_activation(
        &self,
        activation_id: &str,
        request: &ClaimToolActivationRequest,
    ) -> Result<Option<ToolActivationRecord>> {
        self.post_work(
            &format!("v1/tool-activations/{}/claim", encode(activation_id)),
            request,
        )
        .await
    }

    pub async fn plan_tool_activation(
        &self,
        activation_id: &str,
        request: &PlanToolActivationRequest,
    ) -> Result<ToolActivationRecord> {
        self.post_work(
            &format!("v1/tool-activations/{}/plan", encode(activation_id)),
            request,
        )
        .await
    }

    pub async fn update_tool_activation_target(
        &self,
        activation_id: &str,
        target_id: &str,
        request: &UpdateToolActivationTargetRequest,
    ) -> Result<ToolActivationRecord> {
        self.post_work(
            &format!(
                "v1/tool-activations/{}/targets/{}",
                encode(activation_id),
                encode(target_id)
            ),
            request,
        )
        .await
    }

    pub async fn finish_tool_activation(
        &self,
        activation_id: &str,
        request: &FinishToolActivationRequest,
    ) -> Result<ToolActivationRecord> {
        self.post_work(
            &format!("v1/tool-activations/{}/finish", encode(activation_id)),
            request,
        )
        .await
    }

    pub async fn repair_tool_activations(&self) -> Result<usize> {
        self.post_work("v1/tool-activations/repair", &()).await
    }
}

fn encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}
