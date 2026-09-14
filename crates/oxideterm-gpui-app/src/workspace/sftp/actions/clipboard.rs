use super::*;

impl WorkspaceApp {
    pub(in crate::workspace::sftp) fn set_remote_sftp_clipboard(
        &mut self,
        operation: SftpRemoteClipboardOperation,
        cx: &mut Context<Self>,
    ) {
        let Some(remote_id) = self.visible_sftp_remote_id(cx) else {
            return;
        };
        let files = {
            let sftp = self.sftp_view.read(cx);
            sftp.remote_files
                .iter()
                .filter(|file| file.name != ".." && sftp.remote_selected.contains(&file.name))
                .cloned()
                .collect::<Vec<_>>()
        };
        let trace_id = crate::logging::next_audit_trace_id();
        if files.is_empty() {
            tracing::warn!(
                target: "oxideterm::audit",
                trace_id,
                stage = "sftp.remote_clipboard.request",
                operation = operation.audit_name(),
                result = "rejected",
                reason = "no selectable remote files were selected",
                business_impact = "the remote clipboard was not changed",
                "remote clipboard request was rejected before backend work"
            );
            return;
        }
        let selected_count = files.len();
        self.sftp_view.update(cx, |sftp, cx| {
            sftp.remote_clipboard = Some(SftpRemoteClipboard {
                remote_id,
                operation,
                files,
            });
            cx.notify();
        });
        tracing::debug!(
            target: "oxideterm::audit",
            trace_id,
            stage = "sftp.remote_clipboard.request",
            operation = operation.audit_name(),
            selected_count,
            result = "accepted",
            business_impact = "the selected remote entries are ready to paste",
            "remote clipboard request was accepted"
        );
    }

    pub(in crate::workspace::sftp) fn paste_remote_sftp_clipboard(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(remote_id) = self.visible_sftp_remote_id(cx) else {
            return;
        };
        let Some((clipboard, destination_directory)) = ({
            let sftp = self.sftp_view.read(cx);
            sftp.remote_clipboard
                .clone()
                .map(|clipboard| (clipboard, sftp.remote_path.clone()))
        }) else {
            return;
        };
        let trace_id = format!("sftp-paste-{}", uuid::Uuid::new_v4());
        if clipboard.remote_id != remote_id {
            tracing::warn!(
                target: "oxideterm::audit",
                trace_id = %trace_id,
                stage = "sftp.remote_paste.validation",
                operation = clipboard.operation.audit_name(),
                result = "rejected",
                reason = "clipboard and destination belong to different remote connections",
                business_impact = "no remote files were changed",
                "remote paste was rejected before backend work"
            );
            self.push_sftp_toast(
                self.i18n.t("sftp.toast.paste_failed"),
                Some(self.i18n.t("sftp.toast.paste_different_host")),
                TerminalNoticeVariant::Error,
                cx,
            );
            return;
        }
        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
            return;
        };
        let operation = clipboard.operation;
        let files = clipboard.files;
        let total = files.len().max(1) as u64;
        let progress_title = self.i18n.t(match operation {
            SftpRemoteClipboardOperation::Copy => "sftp.toast.copying",
            SftpRemoteClipboardOperation::Cut => "sftp.toast.moving",
        });
        let success_title = self.i18n.t(match operation {
            SftpRemoteClipboardOperation::Copy => "sftp.toast.copy_complete",
            SftpRemoteClipboardOperation::Cut => "sftp.toast.move_complete",
        });
        let success_description = self
            .i18n
            .t("sftp.toast.pasted_count")
            .replace("{{count}}", &files.len().to_string());
        let error_title = self.i18n.t("sftp.toast.paste_failed");
        let error_description = self.i18n.t("sftp.toast.paste_failed_detail");
        let tx = self.sftp_view.read(cx).worker_sender();
        let _ = tx.send(SftpWorkerResult::RemoteMutationProgress {
            key: trace_id.clone(),
            title: progress_title.clone(),
            completed: 0,
            total,
        });
        if operation == SftpRemoteClipboardOperation::Cut {
            // A cut clipboard is consumed when paste starts, preventing stale
            // source paths from being retried after a partial remote move.
            self.sftp_view.update(cx, |sftp, cx| {
                sftp.remote_clipboard = None;
                cx.notify();
            });
        }
        tracing::debug!(
            target: "oxideterm::audit",
            trace_id = %trace_id,
            stage = "sftp.remote_paste.request",
            operation = operation.audit_name(),
            selected_count = files.len(),
            result = "accepted",
            business_impact = "remote-side file work has started without local staging",
            "remote paste request entered backend execution"
        );

        let runtime = self.forwarding_runtime.clone();
        // The workspace runtime owns this bounded operation; it acquires the
        // existing node-backed consumer and never opens an unmanaged transport.
        runtime.spawn(async move {
            let result = async {
                let sftp = backend.acquire_transfer_sftp().await?;
                let connection = if operation == SftpRemoteClipboardOperation::Copy {
                    Some(backend.resolve_connection().await?)
                } else {
                    None
                };
                for (index, file) in files.iter().enumerate() {
                    let destination_path = join_sftp_path(&destination_directory, &file.name);
                    match sftp.stat(&destination_path).await {
                        Ok(_) => return Err(error_description.clone()),
                        Err(SftpError::FileNotFound(_) | SftpError::DirectoryNotFound(_)) => {}
                        Err(_) => return Err(error_description.clone()),
                    }
                    tracing::debug!(
                        target: "oxideterm::audit",
                        trace_id = %trace_id,
                        stage = "sftp.remote_paste.item",
                        operation = operation.audit_name(),
                        item_index = index + 1,
                        item_total = files.len(),
                        result = "started",
                        "remote paste started one selected entry"
                    );
                    let planned_copy_command = plan_remote_copy_command(
                        &file.path,
                        &destination_path,
                        file.file_type == SftpFileType::Directory,
                    )
                    .map_err(|_| error_description.clone())?;
                    match operation {
                        SftpRemoteClipboardOperation::Copy => {
                            let output = connection
                                .as_ref()
                                .expect("copy resolves the existing SSH connection")
                                .run_command_capture(
                                    &planned_copy_command,
                                    std::time::Duration::from_secs(300),
                                    4 * 1024,
                                )
                                .await
                                .map_err(|_| error_description.clone())?;
                            if output.exit_code != Some(0) {
                                return Err(error_description.clone());
                            }
                        }
                        SftpRemoteClipboardOperation::Cut => {
                            sftp.rename(&file.path, &destination_path)
                                .await
                                .map_err(|_| error_description.clone())?;
                        }
                    }
                    let completed = index as u64 + 1;
                    let _ = tx.send(SftpWorkerResult::RemoteMutationProgress {
                        key: trace_id.clone(),
                        title: progress_title.clone(),
                        completed,
                        total,
                    });
                    tracing::debug!(
                        target: "oxideterm::audit",
                        trace_id = %trace_id,
                        stage = "sftp.remote_paste.item",
                        operation = operation.audit_name(),
                        item_index = index + 1,
                        item_total = files.len(),
                        result = "completed",
                        "remote paste completed one selected entry"
                    );
                }
                Ok(())
            }
            .await;
            tracing::debug!(
                target: "oxideterm::audit",
                trace_id = %trace_id,
                stage = "sftp.remote_paste.response",
                operation = operation.audit_name(),
                result = if result.is_ok() { "completed" } else { "failed" },
                failure_detail_redacted = result.is_err(),
                business_impact = if result.is_ok() {
                    "the selected entries were pasted on the remote host"
                } else {
                    "remote paste stopped and the destination listing will refresh"
                },
                "remote paste reached a terminal response"
            );
            let _ = tx.send(SftpWorkerResult::RemoteMutationComplete {
                result,
                refresh_remote: true,
                refresh_local: false,
                toast: Some(SftpMutationToast {
                    success_title,
                    success_description: Some(success_description),
                    error_title,
                }),
                progress_key: Some(trace_id),
            });
        });
    }
}
