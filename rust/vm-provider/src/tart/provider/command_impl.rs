use super::TartProvider;
use crate::{shell_session, CommandProvider, ExecOptions, GuestOutput, LogRecord, VmError};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{error, info};
use vm_core::command_stream::stream_command;
use vm_core::error::Result;
use vm_core::msg;
use vm_messages::messages::MESSAGES;

impl TartProvider {
    pub(super) fn file_upload_command(
        &self,
        local: &str,
        vm_name: &str,
        remote: &str,
    ) -> duct::Expression {
        let command = format!("cat > {}", shell_session::quote_posix_argument(remote));
        self.tart_expr(&["exec", "-i", vm_name, "sh", "-c", &command])
            .stdin_path(local)
    }

    fn app_log_path(&self, container: Option<&str>) -> Result<PathBuf> {
        let vm_name = self.vm_name_with_instance(container)?;
        let tart_home = self.tart_home().map(PathBuf::from).map_or_else(
            || vm_core::user_paths::home_dir().map(|home| home.join(".tart")),
            Ok,
        )?;
        Ok(tart_home.join("vms").join(vm_name).join("app.log"))
    }
}

impl CommandProvider for TartProvider {
    fn ssh(&self, container: Option<&str>, relative_path: &Path) -> Result<()> {
        self.open_shell(container, relative_path)
    }

    fn exec(&self, container: Option<&str>, cmd: &[String]) -> Result<()> {
        let args = self.guest_exec_args(container, cmd)?;
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.stream_tart_command(&arg_refs)
    }

    fn exec_status(&self, container: Option<&str>, cmd: &[String]) -> Result<crate::GuestExit> {
        self.exec_status_with_options(container, cmd, &ExecOptions::default())
    }

    fn exec_status_with_options(
        &self,
        container: Option<&str>,
        cmd: &[String],
        options: &ExecOptions,
    ) -> Result<crate::GuestExit> {
        let args = self.guest_exec_args_with_options(container, cmd, options)?;
        crate::guest_exit::run_guest_command(self.tart().command().args(args))
    }

    fn exec_capture_with_options(
        &self,
        container: Option<&str>,
        cmd: &[String],
        options: &ExecOptions,
    ) -> Result<GuestOutput> {
        let args = self.guest_exec_args_with_options(container, cmd, options)?;
        crate::guest_exit::capture_guest_command(self.tart().command().args(args))
    }

    fn exec_interactive(
        &self,
        container: Option<&str>,
        working_dir: &Path,
        cmd: &[String],
    ) -> Result<()> {
        self.open_interactive_command(container, working_dir, cmd)
    }

    fn exec_with_stdin(&self, container: Option<&str>, cmd: &[String], input: &[u8]) -> Result<()> {
        let mut args = self.guest_exec_args(container, cmd)?;
        args.insert(1, "-i".to_string());
        self.tart_expr(&args)
            .stdin_bytes(input.to_vec())
            .run()
            .map(|_| ())
            .map_err(|_| VmError::Provider("Tart guest command with standard input failed".into()))
    }

    fn exec_output(&self, container: Option<&str>, cmd: &[String]) -> Result<String> {
        let args = self.guest_exec_args(container, cmd)?;
        self.tart_expr(&args)
            .stderr_capture()
            .read()
            .map_err(|error| {
                VmError::Provider(format!(
                    "Failed to capture Tart guest command output: {error}"
                ))
            })
    }

    fn logs(&self, container: Option<&str>) -> Result<()> {
        let vm_name = self.vm_name_with_instance(container)?;
        let log_path = self.app_log_path(container)?;

        if !log_path.exists() {
            let error_msg = format!("Log file not found at: {}", log_path.display());
            error!("{}", error_msg);
            info!("{}", MESSAGES.service.provider_logs_unavailable);
            info!(
                "{}",
                msg!(
                    MESSAGES.service.provider_logs_expected_location,
                    name = vm_name
                )
            );
            return Err(VmError::Internal(error_msg));
        }

        info!(
            "{}",
            msg!(
                MESSAGES.service.provider_logs_showing,
                path = log_path.display().to_string()
            )
        );
        info!("{}", MESSAGES.common.press_ctrl_c_to_stop);

        let log_path = log_path.to_string_lossy();
        stream_command("tail", &["-f", &log_path])
    }

    fn logs_extended(
        &self,
        container: Option<&str>,
        follow: bool,
        tail: usize,
        service: Option<&str>,
        config: &vm_config::config::VmConfig,
    ) -> Result<()> {
        self.logs_records(container, follow, tail, service, config, &mut |record| {
            let mut output: Box<dyn Write> = match record.stream {
                "stderr" => Box::new(io::stderr().lock()),
                _ => Box::new(io::stdout().lock()),
            };
            output.write_all(&record.bytes)?;
            output.flush()?;
            Ok(())
        })
    }

    fn logs_records(
        &self,
        container: Option<&str>,
        follow: bool,
        tail: usize,
        service: Option<&str>,
        _config: &vm_config::config::VmConfig,
        sink: &mut dyn FnMut(LogRecord) -> Result<()>,
    ) -> Result<()> {
        if service.is_some() {
            return Err(VmError::Provider(
                "Tart logs do not support --service".into(),
            ));
        }
        let path = self.app_log_path(container)?;
        if !path.exists() {
            return Err(VmError::Provider(format!(
                "Log file not found at: {}",
                path.display()
            )));
        }
        let mut command = Command::new("tail");
        command.args(["-n", &tail.to_string()]);
        if follow {
            command.arg("-f");
        }
        command.arg(path);
        crate::log_stream::stream_log_command(&mut command, false, sink)
    }

    fn copy(&self, source: &str, destination: &str, container: Option<&str>) -> Result<()> {
        let vm_name = self.vm_name_with_instance(container)?;
        let (local_path, remote_path, is_upload) = if source.contains(':') {
            let parts: Vec<&str> = source.splitn(2, ':').collect();
            if parts.len() == 2 {
                (destination, parts[1], false)
            } else {
                return Err(VmError::Provider("Invalid source format".to_string()));
            }
        } else if destination.contains(':') {
            let parts: Vec<&str> = destination.splitn(2, ':').collect();
            if parts.len() == 2 {
                (source, parts[1], true)
            } else {
                return Err(VmError::Provider("Invalid destination format".to_string()));
            }
        } else {
            (source, destination, true)
        };

        if is_upload {
            self.file_upload_command(local_path, &vm_name, remote_path)
                .run()
                .map_err(|error| {
                    VmError::Provider(format!("Failed to copy file to VM: {error}"))
                })?;
        } else {
            let copy_cmd = format!("cat {}", shell_session::quote_posix_argument(remote_path));
            let result = self
                .tart_expr(&["exec", &vm_name, "sh", "-c", &copy_cmd])
                .stdout_capture()
                .run()
                .map_err(|e| VmError::Provider(format!("Failed to read file from VM: {}", e)))?;

            std::fs::write(local_path, result.stdout)
                .map_err(|e| VmError::Provider(format!("Failed to write local file: {}", e)))?;
        }

        Ok(())
    }
}
