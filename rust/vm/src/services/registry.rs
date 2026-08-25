use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use super::{auth_proxy::AuthProxyService, container::ManagedContainerService, ManagedService};

pub(super) type Services = HashMap<String, Arc<dyn ManagedService>>;

pub(super) fn managed_services() -> Services {
    let shutdown_handles = Arc::new(Mutex::new(HashMap::new()));
    let mut services = HashMap::from([(
        "auth_proxy".to_string(),
        Arc::new(AuthProxyService::new(shutdown_handles)) as Arc<dyn ManagedService>,
    )]);
    services.extend(ManagedContainerService::ALL.map(|service| {
        (
            service.service_name().to_string(),
            Arc::new(service) as Arc<dyn ManagedService>,
        )
    }));
    services
}
