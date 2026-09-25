//! PostgreSQL commands used by backup and restore operations.

use super::DbRoute;
use crate::error::{VmError, VmResult};

/// Execute a command in the selected PostgreSQL service container.
pub(super) async fn execute_docker_command(
    route: &DbRoute,
    args: &[&str],
    input: Option<&[u8]>,
) -> VmResult<Vec<u8>> {
    let mut cmd = tokio::process::Command::new(&route.engine);
    cmd.arg("exec").arg("-i").arg(&route.container);
    cmd.args(args);

    if input.is_some() {
        cmd.stdin(std::process::Stdio::piped());
    }
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| VmError::general(e, "Failed to spawn docker command"))?;

    if let (Some(input_data), Some(mut stdin)) = (input, child.stdin.take()) {
        use tokio::io::AsyncWriteExt;
        if let Err(e) = stdin.write_all(input_data).await {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(VmError::general(
                e,
                "Failed to write to docker command stdin",
            ));
        }
    }

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| VmError::general(e, "Failed to wait for docker command"))?;

    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(VmError::general(
            std::io::Error::new(std::io::ErrorKind::Other, "Docker command failed"),
            String::from_utf8_lossy(&output.stderr),
        ))
    }
}

pub(super) fn quote_pg_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

pub(crate) fn quote_pg_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

async fn execute_admin_sql(route: &DbRoute, query: &str) -> VmResult<Vec<u8>> {
    execute_docker_command(
        route,
        &[
            "psql",
            "-U",
            &route.user,
            "-d",
            "postgres",
            "-v",
            "ON_ERROR_STOP=1",
            "-tA",
            "-c",
            query,
        ],
        None,
    )
    .await
}

async fn database_exists(route: &DbRoute, db_name: &str) -> VmResult<bool> {
    let query = format!(
        "SELECT 1 FROM pg_database WHERE datname = {};",
        quote_pg_literal(db_name)
    );
    let output = execute_admin_sql(route, &query).await?;
    Ok(String::from_utf8_lossy(&output).trim() == "1")
}

async fn disconnect_database(route: &DbRoute, db_name: &str) -> VmResult<()> {
    let query = format!(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = {} AND pid <> pg_backend_pid();",
        quote_pg_literal(db_name)
    );
    execute_admin_sql(route, &query).await?;
    Ok(())
}

pub(super) async fn create_database(route: &DbRoute, db_name: &str) -> VmResult<()> {
    execute_docker_command(route, &["createdb", "-U", &route.user, "--", db_name], None).await?;
    Ok(())
}

pub(super) async fn drop_database(route: &DbRoute, db_name: &str) -> VmResult<()> {
    disconnect_database(route, db_name).await?;
    execute_docker_command(
        route,
        &["dropdb", "-U", &route.user, "--if-exists", "--", db_name],
        None,
    )
    .await?;
    Ok(())
}

async fn rename_database(route: &DbRoute, from: &str, to: &str) -> VmResult<()> {
    let query = format!(
        "ALTER DATABASE {} RENAME TO {};",
        quote_pg_identifier(from),
        quote_pg_identifier(to)
    );
    execute_admin_sql(route, &query).await?;
    Ok(())
}

pub(super) async fn replace_database(
    route: &DbRoute,
    staging_name: &str,
    db_name: &str,
    previous_name: &str,
) -> VmResult<()> {
    let had_previous = match database_exists(route, db_name).await {
        Ok(exists) => exists,
        Err(error) => {
            let _ = drop_database(route, staging_name).await;
            return Err(error);
        }
    };

    if had_previous {
        if let Err(error) = disconnect_database(route, db_name).await {
            let _ = drop_database(route, staging_name).await;
            return Err(error);
        }
        if let Err(error) = rename_database(route, db_name, previous_name).await {
            let _ = drop_database(route, staging_name).await;
            return Err(error);
        }
    }

    if let Err(error) = rename_database(route, staging_name, db_name).await {
        if had_previous {
            if let Err(recovery_error) = rename_database(route, previous_name, db_name).await {
                return Err(VmError::general(
                    recovery_error,
                    format!(
                        "Failed to promote replacement database and to recover original database '{db_name}'"
                    ),
                ));
            }
        }
        let _ = drop_database(route, staging_name).await;
        return Err(error);
    }

    if had_previous {
        if let Err(error) = drop_database(route, previous_name).await {
            vm_core::vm_warning!(
                "Database was replaced, but the previous copy '{}' could not be removed: {}",
                previous_name,
                error
            );
        }
    }

    Ok(())
}
