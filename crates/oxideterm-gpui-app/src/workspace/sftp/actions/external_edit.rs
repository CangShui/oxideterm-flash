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

fn external_edit_temp_path(remote_path: &str) -> Result<PathBuf, String> {
    let file_name = Path::new(remote_path)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "invalid remote file name".to_string())?;
    let dir = std::env::temp_dir().join("oxideterm-external-edit");
    std::fs::create_dir_all(&dir).map_err(|error| format!("{error}"))?;
    let unique = uuid::Uuid::new_v4();
    Ok(dir.join(format!("{unique}-{file_name}")))
}

fn file_signature(path: &Path) -> Option<(SystemTime, u64)> {
    let metadata = std::fs::metadata(path).ok()?;
    Some((
        metadata.modified().ok()?,
        metadata.len(),
    ))
}

impl WorkspaceApp {
    /// MobaXterm-style flow: download the remote content into a temp copy,
    /// open it in the configured external editor, poll for saves, and offer
    /// to upload the changes back to the server.
    pub(in crate::workspace::sftp) fn open_remote_file_in_external_editor(
        &mut self,
        remote_path: String,
        content: String,
        cx: &mut Context<Self>,
    ) {
        let result = self.write_external_edit_temp_copy(&remote_path, &content);
        let Ok((temp_path, signature)) = result else {
            let error = result.unwrap_err();
            self.push_sftp_toast(
                self.i18n.t("sftp.external_edit.prepare_failed"),
                Some(error),
                TerminalNoticeVariant::Error,
                cx,
            );
            return;
        };

        // Launch the configured external editor (empty = system default app).
        let editor = self
            .settings_store
            .settings()
            .general
            .external_editor
            .trim()
            .to_string();
        if editor.is_empty() {
            if let Err(error) = open_path_in_external_app(temp_path.to_string_lossy().as_ref()) {
                self.push_sftp_toast(
                    self.i18n.t("sftp.external_edit.launch_failed"),
                    Some(error.to_string()),
                    TerminalNoticeVariant::Error,
                    cx,
                );
                return;
            }
        } else {
            let spawn = std::process::Command::new(editor.as_str()).arg(&temp_path).spawn();
            if let Err(error) = spawn {
                self.push_sftp_toast(
                    self.i18n.t("sftp.external_edit.launch_failed"),
                    Some(error.to_string()),
                    TerminalNoticeVariant::Error,
                    cx,
                );
                return;
            }
        }

        // Watch the temp copy and offer to upload each detected save.
        self.spawn_external_edit_watch(
            ExternalEditSession { remote_path, temp_path },
            signature,
            cx,
        );
    }

    fn write_external_edit_temp_copy(
        &mut self,
        remote_path: &str,
        content: &str,
    ) -> Result<(PathBuf, Option<(SystemTime, u64)>), String> {
        let temp_path = external_edit_temp_path(remote_path)?;
        std::fs::write(&temp_path, content)
            .map_err(|error| format!("failed to write temp copy: {error}"))?;
        Ok((temp_path.clone(), file_signature(&temp_path)))
    }

    fn spawn_external_edit_watch(
        &mut self,
        session: ExternalEditSession,
        initial_signature: Option<(SystemTime, u64)>,
        cx: &mut Context<Self>,
    ) {
        let started = std::time::Instant::now();
        cx.spawn(async move |this, cx| {
            let mut baseline = initial_signature;
            loop {
                cx.background_executor()
                    .timer(EXTERNAL_EDIT_POLL_INTERVAL)
                    .await;
                if started.elapsed() > EXTERNAL_EDIT_MAX_LIFETIME {
                    break;
                }
                let current: Option<(SystemTime, u64)> =
                    file_signature(&session.temp_path);
                match (&baseline, &current) {
                    (Some(previous), Some(now)) if previous != now => {
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
        })
        .detach();
    }
}

impl WorkspaceApp {

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
        name: String,
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
                let temp_path = temp_path.clone();
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
