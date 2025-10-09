use vm_core::error::Result;

pub fn execute_set(field: String, value: String, global: bool) -> Result<()> {
    crate::config_ops::ConfigOps::set(&field, &value, global, false)
}

pub fn execute_get(field: Option<String>, global: bool) -> Result<()> {
    crate::config_ops::ConfigOps::get(field.as_deref(), global)
}

pub fn execute_unset(field: String, global: bool) -> Result<()> {
    crate::config_ops::ConfigOps::unset(&field, global)
}

pub fn execute_migrate() -> Result<()> {
    crate::config_ops::ConfigOps::migrate()
}
