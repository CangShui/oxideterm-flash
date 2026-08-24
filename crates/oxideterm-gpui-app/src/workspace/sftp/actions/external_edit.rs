use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use super::external::open_path_in_external_app;
use super::*;

// Poll interval for detecting external-editor saves on the temp copy.
const EXTERNAL_EDIT_POLL_INTERVAL: Duration = Duration::from_secs(1);
// Hard lifetime cap so an abandoned editor session cannot leak a poll task.
const EXTERNAL_EDIT_MAX_LIFETIME: Duration = Duration::from_secs(600);

#[derive(Clone)]
pub(crate) struct ExternalEditSession {
    pub remote_path: String,
    pub temp_path: PathBuf,
}

fn external_edit_temp_path(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("oxideterm-external-edit");
    std::fs::create_dir_all(&dir).map_err(|error| format!("{error}"))?;
    let unique = uuid::Uuid::new_v4();
    Ok(dir.join(format!("{unique}-{name}")))
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
        let unique = uuid::Uuid::new_v4();
        let temp_path = temp_dir.join(format!("{unique}-{name}"));
        eprintln!("[ext-edit] 3 backend ok, temp: {}", temp_path.display());
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
            let session = ExternalEditSession { remote_path, temp_path };
            loop {
                cx.background_executor()
                    .timer(EXTERNAL_EDIT_POLL_INTERVAL)
                    .await;
                if started.elapsed() > EXTERNAL_EDIT_MAX_LIFETIME {
                    break;
                }
                let current = file_signature(&session.temp_path);
                match (&baseline, &current) {
                    (Some(previous), Some(now)) if previous != now => {
                        eprintln!("[ext-edit] 11 save detected, prompting upload");
                        baseline = current;
                        this.update(cx, |this, cx| {
                            this.prompt_external_edit_upload(session.clone(), cx);
                        })
                        .ok();
                    }
                    (None, _) => break,
                    _ => {}
                }
            }
            drop(editor);
        })
        .detach();
    }

    fn prompt_external_edit_upload(&mut self, session: ExternalEditSession, cx: &mut Context<Self>) {
        let name = session
            .remote_path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&session.remote_path)
            .to_string();
        self.sftp_view.update(cx, |sftp, cx| {
            sftp.set_dialog(SftpDialog::ExternalEditUploadConfirm {
                name,
                remote_path: session.remote_path.clone(),
                temp_path: session.temp_path.to_string_lossy().to_string(),
            });
            cx.notify();
        });
    }

    /// Uploads the edited temp copy back to the original remote path through
    /// the same remote-mutation channel used by SFTP pane mutations.
    pub(in crate::workspace::sftp) fn confirm_external_edit_upload(
        &mut self,
        _name: String,
        remote_path: String,
        temp_path: String,
        cx: &mut Context<Self>,
    ) {
        self.close_sftp_dialog(cx);
        let toast = crate::workspace::sftp::SftpMutationToast {
            success_title: self.i18n.t("sftp.external_edit.upload_success"),
            success_description: None,
            error_title: self.i18n.t("sftp.external_edit.upload_failed"),
        };
        self.spawn_sftp_pane_remote_mutation(
            crate::workspace::sftp::SftpPane::Remote,
            move |sftp| {
                Box::pin(async move {
                    sftp.upload_file(&temp_path, &remote_path, "external-edit", None, None)
                        .await
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                })
            },
            Some(toast),
            cx,
        );
    }

    /// Discards the pending upload prompt: the temp copy stays so further
    /// saves from the still-open editor re-trigger the prompt.
    pub(in crate::workspace::sftp) fn discard_external_edit_upload(
        &mut self,
        _name: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_sftp_dialog(cx);
    }
}
