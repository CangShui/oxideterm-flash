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
// Hard lifetime cap so an abandoned editor session cannot leak a poll task.
const EXTERNAL_EDIT_MAX_LIFETIME: Duration = Duration::from_secs(600);

#[derive(Clone)]
pub(crate) struct ExternalEditSession {
    backend: SftpRemoteBackend,
    pub remote_path: String,
    pub temp_path: PathBuf,
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
        eprintln!("[ext-edit] 1 enter: {remote_path}");
        let Some(remote_id) = self.visible_sftp_remote_id(cx) else {
            eprintln!("[ext-edit] 1a no remote id");
            return;
        };
        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
            eprintln!("[ext-edit] 2 no backend");
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
        // A dedicated per-session subdirectory avoids stale-ACL issues on a
        // shared folder (create_dir_all skips permission checks when the dir
        // already exists, and the failure then surfaces only at file create).
        let unique = uuid::Uuid::new_v4();
        let session_dir = temp_dir.join(unique.to_string());
        if let Err(error) = std::fs::create_dir_all(&session_dir) {
            eprintln!("[ext-edit] 3a session dir create failed: {error}");
            self.push_sftp_toast(
                self.i18n.t("sftp.external_edit.prepare_failed"),
                Some(error.to_string()),
                TerminalNoticeVariant::Error,
                cx,
            );
            return;
        }
        let temp_path = session_dir.join(&name);
        eprintln!("[ext-edit] 3 backend ok, temp: {}", temp_path.display());
        let session_backend = backend.clone();
        let started = std::time::Instant::now();

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
                    Ok(_) => {
                        eprintln!("[ext-edit] 5 scp download done");
                        Ok(())
                    }
                    Err(error) => {
                        eprintln!("[ext-edit] 5 scp download failed: {error}");
                        Err(error.to_string())
                    }
                }
            };
            let _ = download_tx.send(result.await).await;
            eprintln!("[ext-edit] 6 tokio task exit");
        });

        cx.spawn(async move |this, cx| {
            eprintln!("[ext-edit] 7 gpui: waiting download result");
            if let Err(error) = download_rx.recv().await.unwrap_or_else(|e| Err(e.to_string())) {
                this.update(cx, |this, cx| {
                    this.push_sftp_toast(
                        this.i18n.t("sftp.external_edit.launch_failed"),
                        Some(error),
                        TerminalNoticeVariant::Error,
                        cx,
                    );
                })
                .ok();
                return;
            }

            eprintln!("[ext-edit] 8 launching editor");
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
                    eprintln!("[ext-edit] 9a system-default open");
                    let _ = open_path_in_external_app(&temp_path.to_string_lossy());
                } else {
                    match std::process::Command::new(&editor).arg(&temp_path).spawn() {
                        Ok(_) => eprintln!("[ext-edit] 9b spawned: {editor}"),
                        Err(error) => {
                            eprintln!("[ext-edit] 9b spawn FAILED ({editor}): {error}")
                        }
                    }
                }
            });

            eprintln!("[ext-edit] 10 watching for saves");
            let session = ExternalEditSession {
                backend: session_backend,
                remote_path,
                temp_path,
            };
            'watch: loop {
                cx.background_executor()
                    .timer(EXTERNAL_EDIT_POLL_INTERVAL)
                    .await;
                if started.elapsed() > EXTERNAL_EDIT_MAX_LIFETIME {
                    break;
                }
                let Some(mut stable_signature) = file_signature(&session.temp_path) else {
                    if baseline.is_none() {
                        break;
                    }
                    continue;
                };
                if baseline.as_ref() == Some(&stable_signature) {
                    continue;
                }

                let stable_signature = loop {
                    cx.background_executor()
                        .timer(EXTERNAL_EDIT_SAVE_SETTLE)
                        .await;
                    if started.elapsed() > EXTERNAL_EDIT_MAX_LIFETIME {
                        break 'watch;
                    }
                    let Some(next_signature) = file_signature(&session.temp_path) else {
                        break None;
                    };
                    if next_signature == stable_signature {
                        break Some(stable_signature);
                    }
                    stable_signature = next_signature;
                };
                let Some(stable_signature) = stable_signature else {
                    continue;
                };

                eprintln!("[ext-edit] 11 save detected, uploading");
                let Some(upload_rx) = this
                    .update(cx, |this, _cx| this.start_external_edit_upload(&session))
                    .ok()
                else {
                    break;
                };
                if let Err(error) = upload_rx
                    .recv()
                    .await
                    .unwrap_or_else(|error| Err(error.to_string()))
                {
                    this.update(cx, |this, cx| {
                        this.push_sftp_toast(
                            this.i18n.t("sftp.external_edit.upload_failed"),
                            Some(error),
                            TerminalNoticeVariant::Error,
                            cx,
                        );
                    })
                    .ok();
                }
                // Keep the pre-upload signature so edits made during transfer are
                // observed and uploaded by the next polling cycle.
                baseline = Some(stable_signature);
            }
            drop(editor);
            // Session ended (uploaded or lifetime cap): reclaim the temp copy.
            if let Some(dir) = session.temp_path.parent() {
                std::fs::remove_dir_all(dir).ok();
            }
        })
        .detach();
    }

    fn start_external_edit_upload(
        &self,
        session: &ExternalEditSession,
    ) -> async_channel::Receiver<Result<(), String>> {
        let (upload_tx, upload_rx) = async_channel::bounded::<Result<(), String>>(1);
        let backend = session.backend.clone();
        let remote_path = session.remote_path.clone();
        let temp_path = session.temp_path.to_string_lossy().to_string();
        // The watcher owns the receiver and waits for this one-shot upload result
        // before polling again, keeping saves ordered and the temp file alive.
        self.forwarding_runtime.clone().spawn(async move {
            let result = async {
                let sftp = backend.acquire_transfer_sftp().await?;
                sftp.upload_file(&temp_path, &remote_path, "external-edit", None, None)
                    .await
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            }
            .await;
            if let Err(error) = &result {
                eprintln!("[ext-edit] 12 upload failed: {error}");
            } else {
                eprintln!("[ext-edit] 12 upload done");
            }
            let _ = upload_tx.send(result).await;
        });
        upload_rx
    }
}
