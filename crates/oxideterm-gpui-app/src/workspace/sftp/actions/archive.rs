use super::*;

/// Dropping the UI task aborts the bounded transport future; no archive task survives its owner.
struct ArchiveWorker(tokio::task::JoinHandle<()>);
impl Drop for ArchiveWorker {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl WorkspaceApp {
    pub(in crate::workspace::sftp) fn open_remote_archive_dialog(
        &mut self,
        extract: bool,
        name: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let trace_id = format!("archive-{}", uuid::Uuid::new_v4());
        let Some(remote_id) = self.visible_sftp_remote_id(cx) else {
            return;
        };
        let sftp = self.sftp_view.read(cx);
        let directory = sftp.remote_path.clone();
        let names: Vec<_> = name.map(|name| vec![name]).unwrap_or_else(|| {
            sftp.remote_files
                .iter()
                .filter(|file| file.name != ".." && sftp.remote_selected.contains(&file.name))
                .map(|file| file.name.clone())
                .collect()
        });
        tracing::debug!(target: "oxideterm::audit", trace_id, stage = "archive.dialog.request", extract,
            selected_count = names.len(), "打开远端压缩或解压对话框，不传输文件到本地");
        if names.is_empty()
            || (extract
                && names
                    .iter()
                    .any(|name| oxideterm_sftp::archive_kind(name).is_none()))
        {
            self.push_sftp_toast(
                self.i18n.t("sftp.toast.unsupported_archive"),
                None,
                TerminalNoticeVariant::Error,
                cx,
            );
            return;
        }
        let value = if extract {
            directory.clone()
        } else {
            join_sftp_path(&directory, "archive.tar.gz")
        };
        self.sftp_view.update(cx, |sftp, cx| {
            sftp.dismiss_context_menu(cx);
            sftp.dialog_value = value;
            sftp.set_dialog(SftpDialog::Archive {
                remote_id,
                directory,
                names,
                extract,
                trace_id,
            });
            sftp.focused_input = Some(SftpInput::DialogValue);
            cx.notify();
        });
    }

    pub(in crate::workspace::sftp) fn run_remote_archive(
        &mut self,
        remote_id: SftpRemoteId,
        directory: String,
        names: Vec<String>,
        extract: bool,
        trace_id: String,
        cx: &mut Context<Self>,
    ) {
        let destination = self.sftp_view.read(cx).dialog_value.clone();
        let invalid = !destination.starts_with('/')
            || destination.contains(['\0', '\n', '\r'])
            || destination
                .split('/')
                .any(|part| part == ".." || part == ".");
        if invalid || self.visible_sftp_remote_id(cx).as_ref() != Some(&remote_id) {
            tracing::warn!(target: "oxideterm::audit", trace_id, stage = "archive.validation", "路径无效或对话框已不属于当前远端，操作未启动");
            self.push_sftp_toast(
                self.i18n.t("sftp.archive.invalid"),
                None,
                TerminalNoticeVariant::Error,
                cx,
            );
            return;
        }
        if self.sftp_view.read(cx).archive_task.is_some() {
            self.push_sftp_toast(
                self.i18n.t("sftp.archive.busy"),
                None,
                TerminalNoticeVariant::Error,
                cx,
            );
            return;
        }
        let mut commands = Vec::new();
        let mut tools = Vec::new();
        let planned = if extract {
            names.iter().try_for_each(|name| {
                let path = join_sftp_path(&directory, name);
                let plan = oxideterm_sftp::plan_archive_extraction(name, &path, &destination)
                    .map_err(|_| ())?;
                tools.extend_from_slice(oxideterm_sftp::archive_tools(plan.kind, true));
                commands.push(plan.command);
                Ok(())
            })
        } else {
            oxideterm_sftp::plan_archive_creation(&directory, &names, &destination)
                .map(|(kind, command)| {
                    tools.extend_from_slice(oxideterm_sftp::archive_tools(kind, false));
                    commands.push(command);
                })
                .map_err(|_| ())
        };
        if planned.is_err() {
            self.push_sftp_toast(
                self.i18n.t("sftp.archive.invalid"),
                None,
                TerminalNoticeVariant::Error,
                cx,
            );
            return;
        }
        tools.sort_unstable();
        tools.dedup();
        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
            return;
        };
        let missing = self.i18n.t(if extract {
            "sftp.archive.missing_extract"
        } else {
            "sftp.archive.missing_create"
        });
        let failed = self.i18n.t("sftp.archive.failed");
        let title = self.i18n.t(if extract {
            "sftp.archive.extract"
        } else {
            "sftp.archive.create"
        });
        let toast = SftpMutationToast {
            success_title: self.i18n.t("sftp.archive.done"),
            success_description: None,
            error_title: failed.clone(),
        };
        let tx = self.sftp_view.read(cx).worker_sender();
        let runtime = self.forwarding_runtime.clone();
        self.close_sftp_dialog(cx);
        self.sftp_view.update(cx, |sftp, cx| {
            let task = cx.spawn(async move |weak, cx| {
                let mut worker = ArchiveWorker(runtime.spawn(async move {
                    tracing::info!(target: "oxideterm::audit", trace_id, stage = "archive.execute.request", extract,
                        selected_count = names.len(), tool_count = tools.len(), "执行远端压缩任务，工具缺失时不自动安装");
                    let _ = tx.send(SftpWorkerResult::RemoteMutationProgress { key: trace_id.clone(), title: title.clone(), completed: 0, total: 0 });
                    let result = async {
                        let handle = backend.resolve_connection().await.map_err(|_| failed.clone())?;
                        let check = tools.iter().map(|tool| format!("command -v {tool} >/dev/null 2>&1")).collect::<Vec<_>>().join(" && ");
                        let output = handle.run_command_capture(&check, std::time::Duration::from_secs(15), 1024).await.map_err(|_| failed.clone())?;
                        tracing::info!(target: "oxideterm::audit", trace_id, stage = "archive.tools.response", exit_code = ?output.exit_code, "远端工具检查完成");
                        if output.exit_code != Some(0) { return Err(missing); }
                        if extract {
                            let command = format!("mkdir -p -- {}", oxideterm_sftp::shell_quote(&destination));
                            let output = handle.run_command_capture(&command, std::time::Duration::from_secs(15), 1024).await.map_err(|_| failed.clone())?;
                            if output.exit_code != Some(0) { return Err(failed.clone()); }
                        }
                        for (index, command) in commands.iter().enumerate() {
                            let execution = handle.run_command_capture(command, std::time::Duration::from_secs(1800), 4096);
                            tokio::pin!(execution);
                            let output = loop {
                                tokio::select! {
                                    result = &mut execution => break result.map_err(|_| failed.clone())?,
                                    _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {
                                        let _ = tx.send(SftpWorkerResult::RemoteMutationProgress { key: trace_id.clone(), title: title.clone(), completed: 0, total: 0 });
                                    }
                                }
                            };
                            tracing::info!(target: "oxideterm::audit", trace_id, stage = "archive.item.response", index,
                                exit_code = ?output.exit_code, truncated = output.truncated, "远端压缩命令返回，命令内容和输出不写入日志");
                            if output.exit_code != Some(0) { return Err(failed.clone()); }
                        }
                        Ok(())
                    }.await;
                    tracing::info!(target: "oxideterm::audit", trace_id, stage = "archive.execute.response", succeeded = result.is_ok(), "远端任务结束，刷新远端列表");
                    let _ = tx.send(SftpWorkerResult::RemoteMutationComplete { result, refresh_remote: true, refresh_local: false, toast: Some(toast), progress_key: Some(trace_id) });
                }));
                let _ = (&mut worker.0).await;
                let _ = weak.update(cx, |sftp, cx| { sftp.archive_task = None; cx.notify(); });
            });
            sftp.archive_task = Some(task);
        });
    }
}
