use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use super::{
    auth_proxy::AuthProxyService, mongodb::MongodbService, mysql::MysqlService,
    postgresql::PostgresqlService, redis::RedisService, ManagedService,
};

pub(super) type Services = HashMap<String, Arc<dyn ManagedService>>;

pub(super) fn managed_services() -> Services {
    let shutdown_handles = Arc::new(Mutex::new(HashMap::new()));
    HashMap::from([
        (
            "auth_proxy".to_string(),
            Arc::new(AuthProxyService::new(shutdown_handles)) as Arc<dyn ManagedService>,
        ),
        (
            "postgresql".to_string(),
            Arc::new(PostgresqlService) as Arc<dyn ManagedService>,
        ),
        (
            "redis".to_string(),
            Arc::new(RedisService) as Arc<dyn ManagedService>,
        ),
        (
            "mongodb".to_string(),
            Arc::new(MongodbService) as Arc<dyn ManagedService>,
        ),
        (
            "mysql".to_string(),
            Arc::new(MysqlService) as Arc<dyn ManagedService>,
        ),
    ])
}
