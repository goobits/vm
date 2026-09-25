//! DB utility functions

use super::route::DbRoute;
use crate::error::{VmError, VmResult};

pub async fn execute_psql_command(route: &DbRoute, command: &str) -> VmResult<String> {
    let output = tokio::process::Command::new(&route.engine)
        .arg("exec")
        .arg("-i")
        .arg(&route.container)
        .arg("psql")
        .arg("-U")
        .arg(&route.user)
        .arg("-t") // Tuples only, no headers/footers
        .arg("-A")
        .arg("-c")
        .arg(command)
        .output()
        .await
        .map_err(|e| VmError::general(e, "Failed to execute PostgreSQL command"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(VmError::general(
            std::io::Error::new(std::io::ErrorKind::Other, "Failed to execute psql command."),
            stderr,
        ))
    }
}

pub async fn list_databases(route: &DbRoute) -> VmResult<Vec<String>> {
    let result = execute_psql_command(
        route,
        "SELECT datname FROM pg_database WHERE datistemplate = false AND datname <> 'postgres' ORDER BY datname;",
    )
    .await?;
    Ok(result
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect())
}
