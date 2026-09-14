// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Documented compatibility directory listing for SSH servers without SFTP.
//!
//! OpenWrt/QWRT Dropbear often has no `sftp` subsystem and no `sftp-server`
//! binary. Clients such as MobaXterm still browse files by executing a bounded
//! `ls` on the node-owned SSH connection, then transferring with SCP. This is
//! not native SFTP.

use std::time::Duration;

use russh::ChannelMsg;
use tracing::{debug, info};

use crate::{FileInfo, FileType, SftpError, SftpExecChannelOpener, join_remote_path, shell_quote};

const SHELL_LIST_TIMEOUT: Duration = Duration::from_secs(20);
const SHELL_LIST_MAX_OUTPUT_BYTES: usize = 512 * 1024;
const SHELL_LIST_MAX_ENTRIES: usize = 4_000;

/// Returns whether the error means the remote has no usable SFTP protocol.
pub fn error_is_sftp_protocol_unavailable(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    [
        "sftp subsystem not available",
        "failed to request sftp subsystem",
        "failed to start remote sftp-server",
        "server disabled subsystem",
        "subsystem not available",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

/// Lists one remote directory through a node-owned SSH exec channel.
pub async fn list_remote_directory_via_shell<O>(
    opener: &O,
    path: &str,
) -> Result<(String, Vec<FileInfo>), SftpError>
where
    O: SftpExecChannelOpener,
{
    let command = shell_listing_command(path);
    debug!("listing a remote directory through SSH exec because SFTP is unavailable");
    let output = capture_exec_stdout(opener, &command).await?;
    let (cwd, entries) = parse_shell_listing(&output, path)?;
    info!("opened a remote directory listing through SSH exec compatibility fallback");
    Ok((cwd, entries))
}

fn shell_listing_command(path: &str) -> String {
    let inner = if path.trim().is_empty() {
        "cd \"${HOME:-.}\" 2>/dev/null || cd / || exit 2; pwd; ls -A -l 2>/dev/null || ls -a -l"
            .to_string()
    } else {
        format!(
            "cd -- {} || exit 2; pwd; ls -A -l 2>/dev/null || ls -a -l",
            shell_quote(path.trim())
        )
    };
    format!("/bin/sh -c {}", shell_quote(&inner))
}

async fn capture_exec_stdout<O>(opener: &O, command: &str) -> Result<String, SftpError>
where
    O: SftpExecChannelOpener,
{
    // The exec channel is opened on the node-owned SSH connection. This
    // fallback must never create a second unmanaged transport.
    let mut channel = opener.open_exec_channel().await?;
    channel.exec(true, command).await.map_err(|error| {
        SftpError::ChannelError(format!("Failed to start remote directory listing: {error}"))
    })?;

    let mut output = Vec::new();
    let mut exit_status = None;
    let timed_out = tokio::time::timeout(SHELL_LIST_TIMEOUT, async {
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { data } => {
                    let remaining = SHELL_LIST_MAX_OUTPUT_BYTES.saturating_sub(output.len());
                    output.extend_from_slice(&data[..data.len().min(remaining)]);
                    if output.len() >= SHELL_LIST_MAX_OUTPUT_BYTES {
                        break;
                    }
                }
                ChannelMsg::ExitStatus {
                    exit_status: status,
                } => exit_status = Some(status),
                ChannelMsg::Eof => {}
                ChannelMsg::Close => break,
                _ => {}
            }
        }
    })
    .await
    .is_err();
    let _ = channel.close().await;
    if timed_out {
        return Err(SftpError::ChannelError(
            "Remote directory listing timed out".to_string(),
        ));
    }
    if exit_status == Some(2) {
        return Err(SftpError::DirectoryNotFound(
            "Remote directory could not be opened".to_string(),
        ));
    }
    if let Some(status) = exit_status
        && status != 0
    {
        return Err(SftpError::ChannelError(format!(
            "Remote directory listing exited with status {status}"
        )));
    }
    String::from_utf8(output).map_err(|_| {
        SftpError::ProtocolError("Remote directory listing was not valid UTF-8".to_string())
    })
}

fn parse_shell_listing(
    output: &str,
    requested_path: &str,
) -> Result<(String, Vec<FileInfo>), SftpError> {
    let mut lines = output.lines();
    let cwd = lines
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("total "))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            if requested_path.trim().is_empty() {
                "/".to_string()
            } else {
                requested_path.trim().to_string()
            }
        });
    let mut entries = Vec::new();
    for line in lines {
        if entries.len() >= SHELL_LIST_MAX_ENTRIES {
            break;
        }
        if let Some(entry) = parse_busybox_ls_long_line(line, &cwd) {
            entries.push(entry);
        }
    }
    Ok((cwd, entries))
}

fn parse_busybox_ls_long_line(line: &str, cwd: &str) -> Option<FileInfo> {
    let line = line.trim();
    if line.is_empty() || line.starts_with("total ") {
        return None;
    }
    let perms = line.split_whitespace().next()?;
    if perms.len() < 10 {
        return None;
    }
    let kind = match perms.as_bytes()[0] {
        b'd' => FileType::Directory,
        b'l' => FileType::Symlink,
        b'-' | b'b' | b'c' | b'p' | b's' => FileType::File,
        _ => return None,
    };
    let mut parts = line.split_whitespace();
    let _perms = parts.next()?;
    let _links = parts.next()?;
    let owner = parts.next().map(ToOwned::to_owned);
    let group = parts.next().map(ToOwned::to_owned);
    let size = parts
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    // month, day, time-or-year
    let _month = parts.next()?;
    let _day = parts.next()?;
    let _when = parts.next()?;
    let remainder = parts.collect::<Vec<_>>().join(" ");
    if remainder.is_empty() {
        return None;
    }
    let (name, symlink_target) = if kind == FileType::Symlink {
        remainder
            .split_once(" -> ")
            .map(|(name, target)| (name.to_string(), Some(target.to_string())))
            .unwrap_or((remainder, None))
    } else {
        (remainder, None)
    };
    if name == "." || name == ".." {
        return None;
    }
    Some(FileInfo {
        path: join_remote_path(cwd, &name),
        name,
        file_type: kind,
        size,
        modified: 0,
        permissions: unix_mode_from_ls_perms(perms),
        owner,
        group,
        is_symlink: kind == FileType::Symlink,
        symlink_target,
    })
}

fn unix_mode_from_ls_perms(perms: &str) -> String {
    if perms.len() < 10 {
        return "000".to_string();
    }
    let mut mode = 0u32;
    for (index, flag) in perms[1..10].chars().enumerate() {
        if flag != '-' {
            mode |= 1 << (8 - index);
        }
    }
    format!("{mode:o}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_openwrt_busybox_listing_lines() {
        let cwd = "/root";
        let directory = parse_busybox_ls_long_line(
            "drwxr-xr-x    2 root     root          4096 Jan  1  1970 overlay",
            cwd,
        )
        .expect("directory");
        assert_eq!(directory.name, "overlay");
        assert_eq!(directory.file_type, FileType::Directory);
        assert_eq!(directory.permissions, "755");

        let file = parse_busybox_ls_long_line(
            "-rw-r--r--    1 root     root           481 Jan  1 00:00 passwd",
            cwd,
        )
        .expect("file");
        assert_eq!(file.name, "passwd");
        assert_eq!(file.size, 481);
        assert_eq!(file.permissions, "644");

        let link = parse_busybox_ls_long_line(
            "lrwxrwxrwx    1 root     root             8 Jan  1  1970 tmp -> /tmp",
            cwd,
        )
        .expect("symlink");
        assert_eq!(link.name, "tmp");
        assert_eq!(link.symlink_target.as_deref(), Some("/tmp"));
        assert!(link.is_symlink);
    }

    #[test]
    fn skips_total_and_dot_entries() {
        assert!(parse_busybox_ls_long_line("total 16", "/").is_none());
        assert!(
            parse_busybox_ls_long_line(
                "drwxr-xr-x    2 root     root          4096 Jan  1  1970 .",
                "/"
            )
            .is_none()
        );
    }

    #[test]
    fn shell_listing_command_quotes_the_requested_path() {
        let command = shell_listing_command("/tmp/a'b");
        assert!(command.starts_with("/bin/sh -c "));
        assert!(command.contains("/tmp/a"));
        assert!(command.contains("b"));
    }

    #[test]
    fn sftp_unavailable_classifier_matches_dropbear_failures() {
        assert!(error_is_sftp_protocol_unavailable(
            "SFTP subsystem not available: Failed to request SFTP subsystem: channel failure"
        ));
        assert!(error_is_sftp_protocol_unavailable(
            "SFTP subsystem not available: server disabled subsystem"
        ));
        assert!(!error_is_sftp_protocol_unavailable(
            "Permission denied: /root/secret"
        ));
    }

    #[test]
    fn parse_shell_listing_reads_pwd_then_entries() {
        let output =
            "/root\ntotal 8\ndrwxr-xr-x    2 root     root          4096 Jan  1  1970 overlay\n";
        let (cwd, entries) = parse_shell_listing(output, "").expect("listing");
        assert_eq!(cwd, "/root");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "overlay");
    }
}
