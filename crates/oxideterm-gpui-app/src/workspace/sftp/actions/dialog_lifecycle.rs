use crate::workspace::sftp::helpers::{localized_sftp_error_detail, sftp_error_i18n_key};

use super::*;

impl WorkspaceApp {
    fn spawn_remote_sftp_mutation<F>(
        &self,
        operation: F,
        toast: Option<SftpMutationToast>,
        cx: &App,
    ) where
        F: FnOnce(
                SftpSession,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<(), String>> + Send>,
            > + Send
            + 'static,
    {
        self.spawn_sftp_pane_remote_mutation(SftpPane::Remote, operation, toast, cx);
    }

    pub(in crate::workspace::sftp) fn spawn_sftp_pane_remote_mutation<F>(
        &self,
        pane: SftpPane,
        operation: F,
        toast: Option<SftpMutationToast>,
        cx: &App,
    ) where
        F: FnOnce(
                SftpSession,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<(), String>> + Send>,
            > + Send
            + 'static,
    {
        let trace_id = crate::logging::next_audit_trace_id();
        let remote_id = match pane {
            SftpPane::Local => self.sftp_pair_primary_remote_id(cx),
            SftpPane::Remote => self.visible_sftp_remote_id(cx),
        };
        let Some(remote_id) = remote_id else {
            tracing::warn!(
                target: "oxideterm::audit",
                trace_id,
                stage = "sftp-mutation/validation",
                pane = ?pane,
                result = "rejected",
                reason = "no remote connection is available for the selected pane",
                business_impact = "the requested file operation did not start",
                "SFTP 文件操作在进入后端前被拒绝"
            );
            return;
        };
        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
            tracing::warn!(
                target: "oxideterm::audit",
                trace_id,
                stage = "sftp-mutation/validation",
                pane = ?pane,
                result = "rejected",
                reason = "the managed remote backend is unavailable",
                business_impact = "the requested file operation did not start",
                "SFTP 文件操作在进入后端前被拒绝"
            );
            return;
        };
        let tx = self.sftp_view.read(cx).worker_sender();
        let runtime = self.forwarding_runtime.clone();
        tracing::debug!(
            target: "oxideterm::audit",
            trace_id,
            stage = "sftp-mutation/request",
            pane = ?pane,
            result = "accepted",
            business_impact = "the requested remote file operation entered backend execution",
            "SFTP 文件操作请求已被接受"
        );
        runtime.spawn(async move {
            let result = async {
                let sftp = backend
                    .acquire_transfer_sftp()
                    .await
                    .map_err(|error| error.to_string())?;
                tracing::Instrument::instrument(
                    operation(sftp),
                    tracing::debug_span!(target: "oxideterm::audit", "sftp.mutation", trace_id),
                ).await
            }
            .await;
            tracing::debug!(
                target: "oxideterm::audit",
                trace_id,
                stage = "sftp-mutation/response",
                pane = ?pane,
                result = if result.is_ok() { "completed" } else { "failed" },
                failure_detail_redacted = result.is_err(),
                business_impact = if result.is_ok() {
                    "the requested remote file operation completed"
                } else {
                    "the requested remote file operation did not complete"
                },
                "SFTP 文件操作已返回最终结果"
            );
            let _ = tx.send(SftpWorkerResult::RemoteMutationComplete {
                result,
                refresh_remote: pane == SftpPane::Remote,
                refresh_local: pane == SftpPane::Local,
                toast,
                progress_key: None,
            });
        });
    }

    pub(in crate::workspace::sftp) fn push_sftp_toast(
        &self,
        title: String,
        description: Option<String>,
        variant: TerminalNoticeVariant,
        cx: &App,
    ) {
        let description = if variant == TerminalNoticeVariant::Error {
            description.map(|error| {
                let localized = localized_sftp_error_detail(&self.i18n, &error);
                tracing::debug!(
                    target: "oxideterm::audit",
                    trace_id = crate::logging::next_audit_trace_id(),
                    stage = "sftp-error-localization/response",
                    matched_error_key = sftp_error_i18n_key(&error).unwrap_or("none"),
                    raw_detail_redacted = true,
                    result = "localized",
                    business_impact = "the user receives a readable file-operation error",
                    "已准备用于显示的 SFTP 错误信息"
                );
                localized
            })
        } else {
            description
        };
        self.push_workspace_notice(
            TerminalNotice {
                title,
                description,
                status_text: None,
                progress: None,
                variant,
            },
            cx,
        );
    }

    pub(in crate::workspace::sftp) fn close_sftp_dialog(&mut self, cx: &mut Context<Self>) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        if self
            .sftp_view
            .update(cx, |sftp, cx| sftp.begin_dialog_exit(delay, cx))
        {
            self.ime_marked_text = None;
        }
    }

    pub(in crate::workspace::sftp) fn stop_sftp_preview_media(&mut self, cx: &mut Context<Self>) {
        self.sftp_view
            .update(cx, |sftp, _cx| sftp.stop_preview_media());
    }

    pub(in crate::workspace::sftp) fn toggle_sftp_preview_audio(&mut self, cx: &mut Context<Self>) {
        self.sftp_view
            .update(cx, |sftp, cx| sftp.toggle_preview_audio(cx));
    }

    pub(in crate::workspace::sftp) fn seek_sftp_preview_audio(
        &mut self,
        position: std::time::Duration,
        cx: &mut Context<Self>,
    ) {
        self.sftp_view
            .update(cx, |sftp, cx| sftp.seek_preview_audio(position, cx));
    }

    pub(in crate::workspace::sftp) fn accept_sftp_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.sftp_view.read(cx).dialog() else {
            return;
        };
        match dialog {
            SftpDialog::Archive { remote_id, directory, names, extract, trace_id } => {
                self.run_remote_archive(remote_id, directory, names, extract, trace_id, cx);
            }
            SftpDialog::Rename { pane, old_name } => {
                crate::logging::audit_button_click("sftp-rename-confirm", "sftp");
                let new_name = self
                    .sftp_view
                    .read(cx)
                    .input_value(SftpInput::DialogValue)
                    .trim()
                    .to_string();
                if pane == SftpPane::Local
                    && self.sftp_pair_primary_remote_id(cx).is_some()
                    && !new_name.is_empty()
                {
                    let old_path = {
                        let sftp = self.sftp_view.read(cx);
                        sftp.local_files
                            .iter()
                            .find(|file| file.name == old_name)
                            .map(|file| file.path.clone())
                            .unwrap_or_else(|| join_sftp_path(&sftp.local_path, &old_name))
                    };
                    let new_path = join_sftp_path(&parent_path(&old_path, true), &new_name);
                    let toast = SftpMutationToast {
                        success_title: self.i18n.t("sftp.toast.renamed"),
                        success_description: Some(sftp_i18n_rename_detail(
                            self.i18n.t("sftp.toast.renamed_detail"),
                            &old_name,
                            &new_name,
                        )),
                        error_title: self.i18n.t("sftp.toast.rename_failed"),
                    };
                    let localized_error = self.i18n.t("sftp.toast.rename_failed_detail");
                    self.spawn_sftp_pane_remote_mutation(
                        pane,
                        move |sftp| {
                            Box::pin(async move {
                                sftp.rename(&old_path, &new_path)
                                    .await
                                    .map_err(|_| localized_error)
                            })
                        },
                        Some(toast),
                        cx,
                    );
                    self.close_sftp_dialog(cx);
                    return;
                }
                if !new_name.is_empty() {
                    match pane {
                        SftpPane::Local => {
                            let local_path = self.sftp_view.read(cx).local_path.clone();
                            let old_path = join_local_path(&local_path, &old_name);
                            let new_path = join_local_path(&local_path, &new_name);
                            match std::fs::rename(old_path, new_path) {
                                Ok(()) => {
                                    if let Ok(files) = list_local_files(&local_path) {
                                        self.sftp_view.update(cx, |sftp, cx| {
                                            sftp.local_files = files;
                                            cx.notify();
                                        });
                                    }
                                    self.push_sftp_toast(
                                        self.i18n.t("sftp.toast.renamed"),
                                        Some(sftp_i18n_rename_detail(
                                            self.i18n.t("sftp.toast.renamed_detail"),
                                            &old_name,
                                            &new_name,
                                        )),
                                        TerminalNoticeVariant::Success,
                                        cx,
                                    );
                                }
                                Err(error) => {
                                    tracing::warn!(
                                        target: "oxideterm::audit",
                                        trace_id = crate::logging::next_audit_trace_id(),
                                        stage = "sftp.rename.response",
                                        pane = "local",
                                        result = "failed",
                                        error_kind = ?error.kind(),
                                        failure_detail_redacted = true,
                                        business_impact = "the local item kept its original name",
                                        "本地重命名失败，将显示本地化错误提示"
                                    );
                                    self.push_sftp_toast(
                                        self.i18n.t("sftp.toast.rename_failed"),
                                        Some(self.i18n.t("sftp.toast.rename_failed_detail")),
                                        TerminalNoticeVariant::Error,
                                        cx,
                                    );
                                }
                            }
                        }
                        SftpPane::Remote => {
                            let old_path = {
                                let sftp = self.sftp_view.read(cx);
                                sftp.remote_files
                                    .iter()
                                    .find(|file| file.name == old_name)
                                    .map(|file| file.path.clone())
                                    .unwrap_or_else(|| join_sftp_path(&sftp.remote_path, &old_name))
                            };
                            let new_path = join_sftp_path(&parent_path(&old_path, true), &new_name);
                            let toast = SftpMutationToast {
                                success_title: self.i18n.t("sftp.toast.renamed"),
                                success_description: Some(sftp_i18n_rename_detail(
                                    self.i18n.t("sftp.toast.renamed_detail"),
                                    &old_name,
                                    &new_name,
                                )),
                                error_title: self.i18n.t("sftp.toast.rename_failed"),
                            };
                            let localized_error = self.i18n.t("sftp.toast.rename_failed_detail");
                            self.spawn_remote_sftp_mutation(
                                move |sftp| {
                                    Box::pin(async move {
                                        sftp.rename(&old_path, &new_path)
                                            .await
                                            .map_err(|_| localized_error)
                                    })
                                },
                                Some(toast),
                                cx,
                            );
                        }
                    }
                }
            }
            SftpDialog::NewFolder { pane } => {
                let name = self
                    .sftp_view
                    .read(cx)
                    .input_value(SftpInput::DialogValue)
                    .trim()
                    .to_string();
                if pane == SftpPane::Local
                    && self.sftp_pair_primary_remote_id(cx).is_some()
                    && !name.is_empty()
                {
                    let path = join_sftp_path(&self.sftp_view.read(cx).local_path, &name);
                    let toast = SftpMutationToast {
                        success_title: self.i18n.t("sftp.toast.folder_created"),
                        success_description: Some(name),
                        error_title: self.i18n.t("sftp.toast.create_folder_failed"),
                    };
                    self.spawn_sftp_pane_remote_mutation(
                        pane,
                        move |sftp| {
                            Box::pin(async move {
                                sftp.mkdir(&path).await.map_err(|error| error.to_string())
                            })
                        },
                        Some(toast),
                        cx,
                    );
                    self.close_sftp_dialog(cx);
                    return;
                }
                if !name.is_empty() {
                    match pane {
                        SftpPane::Local => {
                            let local_path = self.sftp_view.read(cx).local_path.clone();
                            let path = join_local_path(&local_path, &name);
                            match std::fs::create_dir_all(path) {
                                Ok(()) => {
                                    if let Ok(files) = list_local_files(&local_path) {
                                        self.sftp_view.update(cx, |sftp, cx| {
                                            sftp.local_files = files;
                                            cx.notify();
                                        });
                                    }
                                    self.push_sftp_toast(
                                        self.i18n.t("sftp.toast.folder_created"),
                                        Some(name),
                                        TerminalNoticeVariant::Success,
                                        cx,
                                    );
                                }
                                Err(error) => {
                                    self.push_sftp_toast(
                                        self.i18n.t("sftp.toast.create_folder_failed"),
                                        Some(error.to_string()),
                                        TerminalNoticeVariant::Error,
                                        cx,
                                    );
                                }
                            }
                        }
                        SftpPane::Remote => {
                            let remote_path = self.sftp_view.read(cx).remote_path.clone();
                            let path = join_sftp_path(&remote_path, &name);
                            let toast = SftpMutationToast {
                                success_title: self.i18n.t("sftp.toast.folder_created"),
                                success_description: Some(name),
                                error_title: self.i18n.t("sftp.toast.create_folder_failed"),
                            };
                            self.spawn_remote_sftp_mutation(
                                move |sftp| {
                                    Box::pin(async move {
                                        sftp.mkdir(&path).await.map_err(|error| error.to_string())
                                    })
                                },
                                Some(toast),
                                cx,
                            );
                        }
                    }
                }
            }
            SftpDialog::NewFile { pane } => {
                crate::logging::audit_button_click("sftp-create-file-confirm", "sftp");
                let name = self
                    .sftp_view
                    .read(cx)
                    .input_value(SftpInput::DialogValue)
                    .trim()
                    .to_string();
                if pane == SftpPane::Local
                    && self.sftp_pair_primary_remote_id(cx).is_some()
                    && !name.is_empty()
                {
                    let path = join_sftp_path(&self.sftp_view.read(cx).local_path, &name);
                    let toast = SftpMutationToast {
                        success_title: self.i18n.t("sftp.toast.file_created"),
                        success_description: Some(name),
                        error_title: self.i18n.t("sftp.toast.create_file_failed"),
                    };
                    self.spawn_sftp_pane_remote_mutation(
                        pane,
                        move |sftp| {
                            Box::pin(async move {
                                sftp.create_empty_file(&path)
                                    .await
                                    .map_err(|error| error.to_string())
                            })
                        },
                        Some(toast),
                        cx,
                    );
                    self.close_sftp_dialog(cx);
                    return;
                }
                if !name.is_empty() {
                    match pane {
                        SftpPane::Local => {
                            let local_path = self.sftp_view.read(cx).local_path.clone();
                            let path = join_local_path(&local_path, &name);
                            match std::fs::OpenOptions::new()
                                .write(true)
                                .create_new(true)
                                .open(&path)
                                .map(|_| ())
                            {
                                Ok(()) => {
                                    if let Ok(files) = list_local_files(&local_path) {
                                        self.sftp_view.update(cx, |sftp, cx| {
                                            sftp.local_files = files;
                                            cx.notify();
                                        });
                                    }
                                    self.push_sftp_toast(
                                        self.i18n.t("sftp.toast.file_created"),
                                        Some(name),
                                        TerminalNoticeVariant::Success,
                                        cx,
                                    );
                                }
                                Err(error) => {
                                    self.push_sftp_toast(
                                        self.i18n.t("sftp.toast.create_file_failed"),
                                        Some(error.to_string()),
                                        TerminalNoticeVariant::Error,
                                        cx,
                                    );
                                }
                            }
                        }
                        SftpPane::Remote => {
                            let remote_path = self.sftp_view.read(cx).remote_path.clone();
                            let path = join_sftp_path(&remote_path, &name);
                            let toast = SftpMutationToast {
                                success_title: self.i18n.t("sftp.toast.file_created"),
                                success_description: Some(name),
                                error_title: self.i18n.t("sftp.toast.create_file_failed"),
                            };
                            self.spawn_remote_sftp_mutation(
                                move |sftp| {
                                    Box::pin(async move {
                                        sftp.create_empty_file(&path)
                                            .await
                                            .map_err(|error| error.to_string())
                                    })
                                },
                                Some(toast),
                                cx,
                            );
                        }
                    }
                }
            }
            SftpDialog::Delete { pane, files } => {
                crate::logging::audit_button_click("sftp-delete", "sftp");
                if pane == SftpPane::Local && self.sftp_pair_primary_remote_id(cx).is_some() {
                    let local_files = self.sftp_view.read(cx).local_files.clone();
                    let targets = files
                        .iter()
                        .filter_map(|name| {
                            local_files
                                .iter()
                                .find(|file| file.name == *name)
                                .map(|file| file.path.clone())
                        })
                        .collect::<Vec<_>>();
                    let count = targets.len();
                    let toast = SftpMutationToast {
                        success_title: self.i18n.t("sftp.toast.deleted"),
                        success_description: Some(sftp_i18n_count(
                            self.i18n.t("sftp.toast.deleted_count"),
                            count,
                        )),
                        error_title: self.i18n.t("sftp.toast.delete_failed"),
                    };
                    self.spawn_sftp_pane_remote_mutation(
                        pane,
                        move |sftp| {
                            Box::pin(async move {
                                for path in targets {
                                    sftp.delete_recursive(&path)
                                        .await
                                        .map_err(|error| error.to_string())?;
                                }
                                Ok(())
                            })
                        },
                        Some(toast),
                        cx,
                    );
                    self.close_sftp_dialog(cx);
                    return;
                }
                match pane {
                    SftpPane::Local => {
                        let local_path = self.sftp_view.read(cx).local_path.clone();
                        let count = files.len();
                        let mut result = Ok(());
                        for name in files {
                            let path = join_local_path(&local_path, &name);
                            result = if std::fs::metadata(&path)
                                .is_ok_and(|metadata| metadata.is_dir())
                            {
                                std::fs::remove_dir_all(path)
                            } else {
                                std::fs::remove_file(path)
                            };
                            if result.is_err() {
                                break;
                            }
                        }
                        match result {
                            Ok(()) => {
                                if let Ok(files) = list_local_files(&local_path) {
                                    self.sftp_view.update(cx, |sftp, cx| {
                                        sftp.local_files = files;
                                        cx.notify();
                                    });
                                }
                                self.push_sftp_toast(
                                    self.i18n.t("sftp.toast.deleted"),
                                    Some(sftp_i18n_count(
                                        self.i18n.t("sftp.toast.deleted_count"),
                                        count,
                                    )),
                                    TerminalNoticeVariant::Success,
                                    cx,
                                );
                            }
                            Err(error) => {
                                self.push_sftp_toast(
                                    self.i18n.t("sftp.toast.delete_failed"),
                                    Some(error.to_string()),
                                    TerminalNoticeVariant::Error,
                                    cx,
                                );
                            }
                        }
                    }
                    SftpPane::Remote => {
                        let remote_files = self.sftp_view.read(cx).remote_files.clone();
                        let targets = files
                            .into_iter()
                            .filter_map(|name| {
                                remote_files
                                    .iter()
                                    .find(|file| file.name == name)
                                    .map(|file| file.path.clone())
                            })
                            .collect::<Vec<_>>();
                        let Some(remote_id) = self.visible_sftp_remote_id(cx) else {
                            self.close_sftp_dialog(cx);
                            return;
                        };
                        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
                            self.close_sftp_dialog(cx);
                            return;
                        };
                        let tx = self.sftp_view.read(cx).worker_sender();
                        let runtime = self.forwarding_runtime.clone();
                        let progress_key = format!("sftp-delete-{}", uuid::Uuid::new_v4());
                        let progress_title = self.i18n.t("sftp.toast.deleting");
                        let success_title = self.i18n.t("sftp.toast.deleted");
                        let success_template = self.i18n.t("sftp.toast.deleted_count");
                        let error_title = self.i18n.t("sftp.toast.delete_failed");
                        let initial_total = targets.len().max(1) as u64;
                        let _ = tx.send(SftpWorkerResult::RemoteMutationProgress {
                            key: progress_key.clone(),
                            title: progress_title.clone(),
                            completed: 0,
                            total: initial_total,
                        });
                        tracing::debug!(
                            target: "oxideterm::audit",
                            trace_id = %progress_key,
                            stage = "sftp.delete",
                            selected_count = targets.len(),
                            result = "accepted",
                            "远程递归删除已开始"
                        );
                        runtime.spawn(async move {
                            let completed =
                                std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
                            let result = async {
                                let sftp = backend
                                    .acquire_transfer_sftp()
                                    .await
                                    .map_err(|error| error.to_string())?;
                                let mut total = 0_u64;
                                for path in &targets {
                                    total = total.saturating_add(
                                        sftp.count_recursive(path)
                                            .await
                                            .map_err(|error| error.to_string())?,
                                    );
                                }
                                let _ = tx.send(SftpWorkerResult::RemoteMutationProgress {
                                    key: progress_key.clone(),
                                    title: progress_title.clone(),
                                    completed: 0,
                                    total: total.max(1),
                                });
                                let total = total.max(1);
                                for path in targets {
                                    let completed_for_callback = completed.clone();
                                    let progress_tx = tx.clone();
                                    let progress_key_for_callback = progress_key.clone();
                                    let progress_title_for_callback = progress_title.clone();
                                    let reporter = std::sync::Arc::new(move || {
                                        let completed = completed_for_callback
                                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                                            .saturating_add(1);
                                        // Limit delivery frequency for trees with many entries while
                                        // preserving the final exact count below.
                                        if completed % 16 == 0 {
                                            let _ = progress_tx.send(
                                                SftpWorkerResult::RemoteMutationProgress {
                                                    key: progress_key_for_callback.clone(),
                                                    title: progress_title_for_callback.clone(),
                                                    completed,
                                                    total,
                                                },
                                            );
                                        }
                                    });
                                    sftp.delete_recursive_with_progress(&path, reporter)
                                        .await
                                        .map_err(|error| error.to_string())?;
                                }
                                let deleted = completed.load(std::sync::atomic::Ordering::Relaxed);
                                let _ = tx.send(SftpWorkerResult::RemoteMutationProgress {
                                    key: progress_key.clone(),
                                    title: progress_title.clone(),
                                    completed: deleted,
                                    total,
                                });
                                Ok(deleted)
                            }
                            .await;
                            let (result, toast) = match result {
                                Ok(deleted) => (
                                    Ok(()),
                                    Some(SftpMutationToast {
                                        success_title,
                                        success_description: Some(sftp_i18n_count(
                                            success_template,
                                            deleted.try_into().unwrap_or(usize::MAX),
                                        )),
                                        error_title,
                                    }),
                                ),
                                Err(error) => (
                                    Err(error),
                                    Some(SftpMutationToast {
                                        success_title,
                                        success_description: None,
                                        error_title,
                                    }),
                                ),
                            };
                            let _ = tx.send(SftpWorkerResult::RemoteMutationComplete {
                                result,
                                refresh_remote: true,
                                refresh_local: false,
                                toast,
                                progress_key: Some(progress_key),
                            });
                        });
                    }
                }
                self.clear_sftp_selection(pane, cx);
            }
            SftpDialog::Conflict => {
                self.resolve_sftp_transfer_conflict(SftpConflictResolution::Rename, cx);
                return;
            }
            _ => {}
        }
        self.close_sftp_dialog(cx);
    }
}

pub(in crate::workspace::sftp) fn sftp_i18n_count(template: String, count: usize) -> String {
    template.replace("{{count}}", &count.to_string())
}

fn sftp_i18n_rename_detail(template: String, old_name: &str, new_name: &str) -> String {
    template
        .replace("{{old}}", old_name)
        .replace("{{new}}", new_name)
}
