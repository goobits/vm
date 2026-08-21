use anyhow::Result;

use crate::{PackageDefinition, RegisterPackage};

use super::{PackageInfrastructureClient, PackageInventory};

impl PackageInfrastructureClient {
    pub async fn packages(&self) -> Result<PackageInventory> {
        self.get_json("api/packages").await
    }

    pub async fn register_package(&self, request: &RegisterPackage) -> Result<PackageDefinition> {
        self.post_work("v1/packages", request).await
    }

    pub async fn package_definitions(&self) -> Result<Vec<PackageDefinition>> {
        self.get_work("v1/packages").await
    }

    pub async fn package_definition(&self, name: &str) -> Result<PackageDefinition> {
        let name = url::form_urlencoded::byte_serialize(name.as_bytes()).collect::<String>();
        self.get_work(&format!("v1/packages/{name}")).await
    }
}
