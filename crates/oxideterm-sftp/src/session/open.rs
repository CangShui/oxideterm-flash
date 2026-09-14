const SFTP_SERVER_FALLBACK_COMMANDS: &[&str] = &[
    "/usr/libexec/sftp-server",
    "/usr/libexec/openssh/sftp-server",
    "/usr/lib/sftp-server",
    "/usr/lib/openssh/sftp-server",
    "sftp-server",
];

fn sftp_owned_config() -> russh_sftp::client::Config {
    russh_sftp::client::Config {
        // The live SFTP session owns this shared budget from queue admission
        // through acknowledgement, matching the existing upload in-flight cap.
        max_outbound_inflight_bytes: SFTP_SINGLE_FILE_MAX_INFLIGHT_BYTES,
        ..Default::default()
    }
}

async fn open_russh_sftp_session(
    channel_factory: &SftpChannelFactory,
) -> Result<RusshSftpSession, SftpError> {
    let subsystem_error =
        match start_sftp_session(channel_factory, SftpChannelStart::Subsystem).await {
            Ok(session) => return Ok(session),
            Err(error) => error,
        };
    debug!(
        "SFTP subsystem is unavailable; trying sftp-server exec fallback without opening a second SSH transport"
    );
    for command in SFTP_SERVER_FALLBACK_COMMANDS {
        match start_sftp_session(channel_factory, SftpChannelStart::Exec(command)).await {
            Ok(session) => {
                info!("Opened SFTP through a remote sftp-server program");
                return Ok(session);
            }
            Err(_) => continue,
        }
    }
    Err(subsystem_error)
}

#[derive(Clone, Copy)]
enum SftpChannelStart<'a> {
    Subsystem,
    Exec(&'a str),
}

async fn start_sftp_session(
    channel_factory: &SftpChannelFactory,
    start: SftpChannelStart<'_>,
) -> Result<RusshSftpSession, SftpError> {
    let channel = channel_factory().await?;
    match start {
        SftpChannelStart::Subsystem => {
            channel
                .request_subsystem(true, "sftp")
                .await
                .map_err(|error| {
                    SftpError::SubsystemNotAvailable(format!(
                        "Failed to request SFTP subsystem: {error}"
                    ))
                })?;
        }
        SftpChannelStart::Exec(command) => {
            channel.exec(true, command).await.map_err(|error| {
                SftpError::SubsystemNotAvailable(format!(
                    "Failed to start remote sftp-server: {error}"
                ))
            })?;
        }
    }
    let (reader, writer) = channel.into_stream().into_split();
    RusshSftpSession::new_owned_with_config(
        reader,
        RusshOwnedSftpWriter(writer),
        sftp_owned_config(),
    )
    .await
    .map_err(|error| SftpError::SubsystemNotAvailable(error.to_string()))
}

async fn resolve_initial_remote_cwd(sftp: &RusshSftpSession) -> String {
    for candidate in [".", "", "/"] {
        if let Ok(path) = sftp.canonicalize(candidate).await {
            let path = path.trim();
            if !path.is_empty() {
                return path.to_string();
            }
        }
    }
    "/".to_string()
}

struct RusshOwnedSftpWriter(russh::ChannelStreamWriter<russh::client::Msg>);

impl russh_sftp::client::OwnedSftpWriter for RusshOwnedSftpWriter {
    async fn write_owned(&mut self, data: bytes::Bytes) -> std::io::Result<()> {
        self.0.write_bytes(data).await
    }

    async fn shutdown(&mut self) -> std::io::Result<()> {
        self.0.shutdown().await
    }
}

#[cfg(test)]
mod sftp_open_tests {
    use super::*;

    #[test]
    fn sftp_server_fallback_commands_cover_openwrt_and_openssh_paths() {
        assert!(SFTP_SERVER_FALLBACK_COMMANDS.contains(&"/usr/libexec/sftp-server"));
        assert!(SFTP_SERVER_FALLBACK_COMMANDS.contains(&"sftp-server"));
        assert!(
            SFTP_SERVER_FALLBACK_COMMANDS
                .iter()
                .all(|command| !command.contains(' ') && !command.contains(';'))
        );
    }
}
