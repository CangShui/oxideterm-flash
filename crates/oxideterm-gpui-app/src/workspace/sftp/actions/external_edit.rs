use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use super::external::open_path_in_external_app;
use super::*;

// Poll interval for detecting external-editor saves on the temp copy.
const EXTERNAL_EDIT_POLL_INTERVAL: Duration = Duration::from_secs(1);
// Wait for the editor's save sequence to settle before uploading the final bytes.
const EXTERNAL_EDIT_SAVE_SETTLE: Duration = Duration::from_millis(250);
// A failed apply keeps the local cache and retries without requiring a new save.
const EXTERNAL_EDIT_RETRY_INTERVAL: Duration = Duration::from_secs(15);
const EXTERNAL_EDIT_PENDING_MARKER: &str = ".oxideterm-remote-pending";

pub(crate) struct ExternalEditSession {
    backend: SftpRemoteBackend,
    _connection_lease: ExternalEditConnectionLease,
    pub remote_path: String,
    pub temp_path: PathBuf,
}

struct ExternalEditConnectionLease {
    registry: SshConnectionRegistry,
    connection_id: String,
    consumer: ConnectionConsumer,
    trace_id: u64,
}

impl Drop for ExternalEditConnectionLease {
    fn drop(&mut self) {
        self.registry.release(&self.connection_id, &self.consumer);
        tracing::info!(
            target: "oxideterm::audit",
            trace_id = self.trace_id,
            stage = "sftp-external-edit/connection-owner",
            connection_id = %self.connection_id,
            result = "released",
            "外部编辑会话已结束，释放其独立 SSH 连接消费者"
        );
    }
}

struct ExternalEditCacheGuard {
    directory: PathBuf,
    retain: bool,
}

impl ExternalEditCacheGuard {
    fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            retain: false,
        }
    }

    fn mark_pending(&mut self) {
        self.retain = true;
        let _ = std::fs::write(
            self.directory.join(EXTERNAL_EDIT_PENDING_MARKER),
            b"remote-apply-pending\n",
        );
    }

    fn mark_applied(&mut self) {
        self.retain = false;
        let _ = std::fs::remove_file(self.directory.join(EXTERNAL_EDIT_PENDING_MARKER));
    }
}

impl Drop for ExternalEditCacheGuard {
    fn drop(&mut self) {
        if !self.retain {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }
}

fn cleanup_stale_external_edit_caches(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || path.join(EXTERNAL_EDIT_PENDING_MARKER).exists() {
            continue;
        }
        let _ = std::fs::remove_dir_all(path);
    }
}

fn file_signature(path: &Path) -> Option<(SystemTime, u64)> {
    let metadata = std::fs::metadata(path).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

impl WorkspaceApp {
    /// Entry point for remote SFTP files (MobaXterm style): download to a
    /// temp copy via SCP, launch the configured external editor, poll the
    /// temp copy for saves and offer to upload each change back.
    pub(in crate::workspace::sftp) fn open_sftp_file_in_external_editor(
        &mut self,
        remote_path: String,
        name: String,
        cx: &mut Context<Self>,
    ) {
        let launch_trace_id = crate::logging::next_audit_trace_id();
        tracing::debug!(
            target: "oxideterm::audit",
            trace_id = launch_trace_id,
            stage = "sftp-external-edit/request",
            result = "received",
            business_impact = "the remote file is being prepared for external editing",
            "外部编辑请求已到达工作区"
        );
        let Some(remote_id) = self.visible_sftp_remote_id(cx) else {
            tracing::warn!(
                target: "oxideterm::audit",
                trace_id = launch_trace_id,
                stage = "sftp-external-edit/validation",
                result = "rejected",
                reason = "no visible remote connection",
                business_impact = "the external editor was not opened",
                "外部编辑请求在进入后端前被拒绝"
            );
            return;
        };
        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
            tracing::warn!(
                target: "oxideterm::audit",
                trace_id = launch_trace_id,
                stage = "sftp-external-edit/validation",
                result = "rejected",
                reason = "remote backend unavailable",
                business_impact = "the external editor was not opened",
                "外部编辑请求在进入后端前被拒绝"
            );
            return;
        };
        let temp_dir = std::env::temp_dir().join("oxideterm-external-edit");
        if std::fs::create_dir_all(&temp_dir).is_err() {
            self.push_sftp_toast(
                self.i18n.t("sftp.external_edit.prepare_failed"),
                None,
                TerminalNoticeVariant::Error,
                cx,
            );
            return;
        }
        // A previous forced process termination cannot run task destructors.
        // Remove only caches without a pending marker; unsynchronized edits survive.
        cleanup_stale_external_edit_caches(&temp_dir);
        // A dedicated per-session subdirectory avoids stale-ACL issues on a
        // shared folder (create_dir_all skips permission checks when the dir
        // already exists, and the failure then surfaces only at file create).
        let unique = uuid::Uuid::new_v4();
        let session_dir = temp_dir.join(unique.to_string());
        if let Err(error) = std::fs::create_dir_all(&session_dir) {
            tracing::warn!(
                target: "oxideterm::audit",
                trace_id = launch_trace_id,
                stage = "sftp-external-edit/prepare",
                result = "failed",
                error_kind = ?error.kind(),
                failure_detail_redacted = true,
                business_impact = "the external editor was not opened",
                "无法准备外部编辑的临时工作目录"
            );
            self.push_sftp_toast(
                self.i18n.t("sftp.external_edit.prepare_failed"),
                Some(error.to_string()),
                TerminalNoticeVariant::Error,
                cx,
            );
            return;
        }
        let temp_path = session_dir.join(&name);
        let cache_guard = ExternalEditCacheGuard::new(session_dir);
        let Some(connection_id) = backend.external_edit_connection_id() else {
            self.push_sftp_toast(
                self.i18n.t("sftp.external_edit.launch_failed"),
                Some(self.i18n.t("sftp.errors.connection_unavailable")),
                TerminalNoticeVariant::Error,
                cx,
            );
            return;
        };
        let consumer = ConnectionConsumer::Ide(format!("external-edit:{unique}"));
        let Some(_connection_handle) = self
            .ssh_registry
            .acquire_consumer_for_connection(&connection_id, consumer.clone())
        else {
            self.push_sftp_toast(
                self.i18n.t("sftp.external_edit.launch_failed"),
                Some(self.i18n.t("sftp.errors.connection_unavailable")),
                TerminalNoticeVariant::Error,
                cx,
            );
            return;
        };
        let connection_lease = ExternalEditConnectionLease {
            registry: self.ssh_registry.clone(),
            connection_id: connection_id.clone(),
            consumer,
            trace_id: launch_trace_id,
        };
        tracing::info!(
            target: "oxideterm::audit",
            trace_id = launch_trace_id,
            stage = "sftp-external-edit/connection-owner",
            connection_id,
            result = "acquired",
            "外部编辑会话已注册为独立 SSH 消费者，关闭终端标签不会中断文件同步"
        );
        let session_backend = backend.clone();

        // scp_download_file uses tokio::fs internally, so the download must
        // run on the forwarding Tokio runtime; the GPUI executor has no Tokio
        // reactor. Completion is bridged back through a channel.
        let (download_tx, download_rx) = async_channel::bounded::<Result<(), String>>(1);
        let dl_remote_path = remote_path.clone();
        let dl_temp_path = temp_path.clone();
        self.forwarding_runtime.clone().spawn(async move {
            let remote_path = dl_remote_path;
            let temp_path = dl_temp_path;
            let result = async {
                let resolved = backend
                    .resolve_connection()
                    .await
                    .map_err(|e| e.to_string())?;
                match oxideterm_sftp::scp_download_file(
                    &resolved,
                    &remote_path,
                    &temp_path.to_string_lossy(),
                    LocalDownloadDisposition::CreateNew,
                    "external-edit-download",
                    None,
                    None,
                )
                .await
                {
                    Ok(_) => Ok(()),
                    Err(error) => Err(error.to_string()),
                }
            };
            let _ = download_tx.send(result.await).await;
        });

        let task = cx.spawn(async move |this, cx| {
            let mut cache_guard = cache_guard;
            if let Err(error) = download_rx
                .recv()
                .await
                .unwrap_or_else(|e| Err(e.to_string()))
            {
                this.update(cx, |this, cx| {
                    this.push_sftp_toast(
                        this.i18n.t("sftp.external_edit.launch_failed"),
                        Some(error),
                        TerminalNoticeVariant::Error,
                        cx,
                    );
                })
                .ok();
                tracing::warn!(
                    target: "oxideterm::audit",
                    trace_id = launch_trace_id,
                    stage = "sftp-external-edit/download",
                    result = "failed",
                    failure_detail_redacted = true,
                    business_impact = "the external editor was not opened",
                    "外部编辑下载收到错误响应"
                );
                return;
            }

            let mut baseline = file_signature(&temp_path);
            let editor = this.update(cx, |this, _cx| {
                let editor = this
                    .settings_store
                    .settings()
                    .general
                    .external_editor
                    .trim()
                    .to_string();
                if editor.is_empty() {
                    let _ = open_path_in_external_app(&temp_path.to_string_lossy());
                } else {
                    match std::process::Command::new(&editor).arg(&temp_path).spawn() {
                        Ok(_) => {}
                        Err(error) => tracing::warn!(
                            target: "oxideterm::audit",
                            trace_id = launch_trace_id,
                            stage = "sftp-external-edit/launch",
                            result = "failed",
                            error_kind = ?error.kind(),
                            failure_detail_redacted = true,
                            business_impact = "the configured external editor did not start",
                            "外部编辑器进程启动失败"
                        ),
                    }
                }
            });

            tracing::debug!(
                target: "oxideterm::audit",
                trace_id = launch_trace_id,
                stage = "sftp-external-edit/launch",
                result = "completed",
                business_impact = "the temporary edit session is watching for saved changes",
                "外部编辑器启动阶段完成"
            );
            let session = ExternalEditSession {
                backend: session_backend,
                _connection_lease: connection_lease,
                remote_path,
                temp_path,
            };
            // Local saves are cached immediately; a version stays pending until
            // the remote upload is acknowledged, so disconnects cannot silently
            // discard the edit or claim it was applied.
            let mut pending_remote_apply: Option<(SystemTime, u64)> = None;
            let mut next_retry_at = std::time::Instant::now();
            loop {
                cx.background_executor()
                    .timer(EXTERNAL_EDIT_POLL_INTERVAL)
                    .await;
                let Some(mut stable_signature) = file_signature(&session.temp_path) else {
                    if baseline.is_none() {
                        break;
                    }
                    continue;
                };
                if baseline.as_ref() != Some(&stable_signature) {
                    let settled_signature = loop {
                        cx.background_executor()
                            .timer(EXTERNAL_EDIT_SAVE_SETTLE)
                            .await;
                        let Some(next_signature) = file_signature(&session.temp_path) else {
                            break None;
                        };
                        if next_signature == stable_signature {
                            break Some(stable_signature);
                        }
                        stable_signature = next_signature;
                    };
                    let Some(stable_signature) = settled_signature else {
                        continue;
                    };
                    // A new local version is now cached; tell the user before
                    // any network work so a failure can never look like success.
                    baseline = Some(stable_signature);
                    pending_remote_apply = Some(stable_signature);
                    cache_guard.mark_pending();
                    next_retry_at = std::time::Instant::now();
                    this.update(cx, |this, cx| {
                        this.push_sftp_toast(
                            this.i18n.t("sftp.external_edit.cached_title"),
                            Some(this.i18n_replace(
                                "sftp.external_edit.cached_description",
                                &[("name", name.clone())],
                            )),
                            TerminalNoticeVariant::Default,
                            cx,
                        );
                    })
                    .ok();
                    tracing::info!(
                        target: "oxideterm::audit",
                        trace_id = crate::logging::next_audit_trace_id(),
                        stage = "sftp-external-edit/cache",
                        result = "cached",
                        remote_path_character_count = session.remote_path.chars().count(),
                        business_impact = "the local edit is cached and still pending remote application",
                        "外部编辑器保存已写入本地缓存，等待上传"
                    );
                }

                if !session.backend.external_edit_owner_exists() {
                    tracing::info!(
                        target: "oxideterm::audit",
                        trace_id = crate::logging::next_audit_trace_id(),
                        stage = "sftp-external-edit/owner",
                        pending_remote_apply = pending_remote_apply.is_some(),
                        result = "closed",
                        "SSH/SFTP 运行时所有者已关闭，外部编辑监听任务结束"
                    );
                    break;
                }

                if let Some(uploaded_signature) = pending_remote_apply {
                    if std::time::Instant::now() < next_retry_at {
                        continue;
                    }
                    let Some(upload_rx) = this
                        .update(cx, |this, cx| this.start_external_edit_upload(&session, cx))
                        .ok()
                    else {
                        break;
                    };
                    match upload_rx
                        .recv()
                        .await
                        .unwrap_or_else(|error| Err(error.to_string()))
                    {
                        Ok(()) => {
                            let latest_signature = file_signature(&session.temp_path);
                            let has_newer_local_save =
                                latest_signature.is_some_and(|latest| latest != uploaded_signature);
                            if has_newer_local_save {
                                // The upload applied an older stable version while
                                // the editor saved again. Keep the cache pending.
                                baseline = Some(uploaded_signature);
                                pending_remote_apply = latest_signature;
                                cache_guard.mark_pending();
                                next_retry_at = std::time::Instant::now();
                                continue;
                            }
                            pending_remote_apply = None;
                            cache_guard.mark_applied();
                            this.update(cx, |this, cx| {
                                this.push_sftp_toast(
                                    this.i18n.t("sftp.external_edit.remote_applied_title"),
                                    Some(this.i18n_replace(
                                        "sftp.external_edit.remote_applied_description",
                                        &[("name", name.clone())],
                                    )),
                                    TerminalNoticeVariant::Success,
                                    cx,
                                );
                            })
                            .ok();
                            tracing::info!(
                                target: "oxideterm::audit",
                                trace_id = crate::logging::next_audit_trace_id(),
                                stage = "sftp-external-edit/apply",
                                result = "applied",
                                business_impact = "the cached edit is now present on the remote host",
                                "本地缓存编辑已成功应用到远程文件"
                            );
                        }
                        Err(error) => {
                            // Keep the cache and retry the same version; the user
                            // must see that the remote file was not updated.
                            next_retry_at =
                                std::time::Instant::now() + EXTERNAL_EDIT_RETRY_INTERVAL;
                            let cache_path = session.temp_path.display().to_string();
                            this.update(cx, |this, cx| {
                                this.push_sftp_toast(
                                    this.i18n.t("sftp.external_edit.pending_title"),
                                    Some(this.i18n_replace(
                                        "sftp.external_edit.pending_description",
                                        &[("name", name.clone()), ("path", cache_path.clone())],
                                    )),
                                    TerminalNoticeVariant::Warning,
                                    cx,
                                );
                            })
                            .ok();
                            tracing::warn!(
                                target: "oxideterm::audit",
                                trace_id = crate::logging::next_audit_trace_id(),
                                stage = "sftp-external-edit/apply",
                                result = "pending",
                                retry_after_seconds = EXTERNAL_EDIT_RETRY_INTERVAL.as_secs(),
                                cache_retained = true,
                                failure_detail_redacted = true,
                                business_impact = "the remote file kept its previous content; the local cache will retry",
                                "本地缓存编辑未能应用到远程，保持待同步状态"
                            );
                            drop(error);
                        }
                    }
                }
            }
            drop(editor);
            if pending_remote_apply.is_some() {
                // Never discard an unapplied edit. Keep the cache on disk and
                // tell the user exactly where it is so nothing is silently lost.
                let cache_path = session.temp_path.display().to_string();
                this.update(cx, |this, cx| {
                    this.push_sftp_toast(
                        this.i18n.t("sftp.external_edit.cache_retained_title"),
                        Some(this.i18n_replace(
                            "sftp.external_edit.cache_retained_description",
                            &[("name", name.clone()), ("path", cache_path)],
                        )),
                        TerminalNoticeVariant::Warning,
                        cx,
                    );
                })
                .ok();
                tracing::warn!(
                    target: "oxideterm::audit",
                    trace_id = crate::logging::next_audit_trace_id(),
                    stage = "sftp-external-edit/cleanup",
                    result = "retained",
                    cache_retained = true,
                    business_impact = "the unapplied edit stays on disk instead of being deleted",
                    "外部编辑会话结束，本地缓存已保留"
                );
            }
        })
        ;
        self.sftp_view.update(cx, |sftp, _cx| {
            sftp.external_edit_tasks.push(task);
        });
    }

    fn start_external_edit_upload(
        &self,
        session: &ExternalEditSession,
        cx: &App,
    ) -> async_channel::Receiver<Result<(), String>> {
        let (upload_tx, upload_rx) = async_channel::bounded::<Result<(), String>>(1);
        let backend = session.backend.clone();
        let remote_path = session.remote_path.clone();
        let temp_path = session.temp_path.to_string_lossy().to_string();
        let progress_key = format!("sftp-external-edit-{}", uuid::Uuid::new_v4());
        let transfer_id = progress_key.clone();
        let progress_title = self.i18n.t("sftp.toast.editing");
        let success_title = self.i18n.t("sftp.toast.edit_complete");
        let error_title = self.i18n.t("sftp.toast.edit_failed");
        let total = std::fs::metadata(&session.temp_path)
            .map(|metadata| metadata.len())
            .unwrap_or(0)
            .max(1);
        let worker_tx = self.sftp_view.read(cx).worker_sender();
        let _ = worker_tx.send(SftpWorkerResult::RemoteMutationProgress {
            key: progress_key.clone(),
            title: progress_title.clone(),
            completed: 0,
            total,
        });
        tracing::debug!(
            target: "oxideterm::audit",
            trace_id = %progress_key,
            stage = "sftp-external-edit/upload",
            total_bytes = total,
            result = "started",
            business_impact = "the externally edited file is being uploaded",
            "外部编辑器保存已进入后台上传执行"
        );
        // The watcher owns the receiver and waits for this one-shot upload result
        // before polling again, keeping saves ordered and the temp file alive.
        let runtime = self.forwarding_runtime.clone();
        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<TransferProgress>(32);
        let progress_worker_tx = worker_tx.clone();
        let progress_worker_key = progress_key.clone();
        let progress_worker_title = progress_title.clone();
        // This bridge is bounded by the upload sender; it exits as soon as the
        // node-owned transfer closes its progress channel.
        runtime.spawn(async move {
            while let Some(progress) = progress_rx.recv().await {
                let _ = progress_worker_tx.send(SftpWorkerResult::RemoteMutationProgress {
                    key: progress_worker_key.clone(),
                    title: progress_worker_title.clone(),
                    completed: progress.transferred_bytes,
                    total: progress.total_bytes.max(1),
                });
            }
        });
        runtime.spawn(async move {
            let result = async {
                let sftp = backend.acquire_transfer_sftp().await?;
                sftp.upload_file(
                    &temp_path,
                    &remote_path,
                    &transfer_id,
                    Some(progress_tx),
                    None,
                )
                .await
                .map(|_| ())
                .map_err(|error| error.to_string())
            }
            .await;
            if result.is_ok() {
                let _ = worker_tx.send(SftpWorkerResult::RemoteMutationProgress {
                    key: progress_key.clone(),
                    title: progress_title,
                    completed: total,
                    total,
                });
            }
            tracing::debug!(
                target: "oxideterm::audit",
                trace_id = %progress_key,
                stage = "sftp-external-edit/response",
                result = if result.is_ok() { "completed" } else { "failed" },
                failure_detail_redacted = result.is_err(),
                business_impact = if result.is_ok() {
                    "the externally edited content replaced the remote file"
                } else {
                    "the remote file kept its previous saved content"
                },
                "外部编辑器上传已返回最终结果"
            );
            let _ = worker_tx.send(SftpWorkerResult::RemoteMutationComplete {
                result: result.clone(),
                refresh_remote: result.is_ok(),
                refresh_local: false,
                toast: Some(SftpMutationToast {
                    success_title,
                    success_description: None,
                    error_title,
                }),
                progress_key: Some(progress_key),
            });
            let _ = upload_tx.send(result).await;
        });
        upload_rx
    }
}

#[cfg(test)]
mod external_edit_cache_tests {
    use super::*;

    #[test]
    fn synchronized_cache_is_removed_when_its_owner_drops() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("synced");
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join("file.txt"), b"synced").unwrap();

        drop(ExternalEditCacheGuard::new(directory.clone()));

        assert!(!directory.exists());
    }

    #[test]
    fn pending_cache_survives_owner_drop_and_stale_cleanup() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("pending");
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join("file.txt"), b"not applied").unwrap();
        let mut guard = ExternalEditCacheGuard::new(directory.clone());
        guard.mark_pending();

        drop(guard);
        cleanup_stale_external_edit_caches(root.path());

        assert!(directory.exists());
        assert!(directory.join(EXTERNAL_EDIT_PENDING_MARKER).exists());
    }

    #[test]
    fn startup_cleanup_removes_only_non_pending_process_leftovers() {
        let root = tempfile::tempdir().unwrap();
        let synchronized = root.path().join("synchronized");
        let pending = root.path().join("pending");
        std::fs::create_dir(&synchronized).unwrap();
        std::fs::create_dir(&pending).unwrap();
        std::fs::write(synchronized.join("file.txt"), b"synced").unwrap();
        std::fs::write(pending.join("file.txt"), b"pending").unwrap();
        std::fs::write(
            pending.join(EXTERNAL_EDIT_PENDING_MARKER),
            b"remote-apply-pending\n",
        )
        .unwrap();

        cleanup_stale_external_edit_caches(root.path());

        assert!(!synchronized.exists());
        assert!(pending.exists());
    }

    #[test]
    fn external_edit_lease_keeps_connection_owned_until_editor_task_ends() {
        let registry = SshConnectionRegistry::default();
        let node_consumer = ConnectionConsumer::NodeRouter("node-test".into());
        let handle = registry.acquire(SshConfig::default(), node_consumer.clone());
        let connection_id = handle.connection_id().to_string();
        let editor_consumer = ConnectionConsumer::Ide("external-edit-test".into());
        registry
            .acquire_consumer_for_connection(&connection_id, editor_consumer.clone())
            .unwrap();
        let lease = ExternalEditConnectionLease {
            registry: registry.clone(),
            connection_id: connection_id.clone(),
            consumer: editor_consumer,
            trace_id: 1,
        };

        assert_eq!(registry.get(&connection_id).unwrap().info().ref_count, 2);
        registry.release(&connection_id, &node_consumer);
        assert_eq!(registry.get(&connection_id).unwrap().info().ref_count, 1);
        drop(lease);
        assert_eq!(registry.get(&connection_id).unwrap().info().ref_count, 0);
    }
}
