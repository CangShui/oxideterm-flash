// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Session file export to third-party client formats. Passwords and private
//! keys are never included; only connection parameters travel in the output.
//!
//! Every non-native format mirrors the reader in `connection_import.rs` so an
//! exported file can be imported back by the same client type: SecureCRT uses
//! the sessions XML export, Xshell uses a `.xts` archive, Termius/WindTerm/
//! Electerm use their JSON bookmarks shapes, and FinalShell uses its `conn`
//! directory layout.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{Read, Write as IoWrite};
use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
use uuid::Uuid;

use crate::store::SavedConnection;

/// The set of session file formats the exporter can produce.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionExportFormat {
    OxideEncrypted,
    OxideTermJson,
    SecureCrt,
    Xshell,
    Termius,
    MobaXterm,
    WindTerm,
    Electerm,
    FinalShell,
}

impl SessionExportFormat {
    pub fn tag(self) -> &'static str {
        match self {
            Self::OxideEncrypted => "oxide",
            Self::OxideTermJson => "json",
            Self::SecureCrt => "securecrt",
            Self::Xshell => "xshell",
            Self::Termius => "termius",
            Self::MobaXterm => "mobaxterm",
            Self::WindTerm => "windterm",
            Self::Electerm => "electerm",
            Self::FinalShell => "finalshell",
        }
    }

    /// File extension for single-file formats. `None` for directory exports.
    pub fn file_extension(self) -> Option<&'static str> {
        match self {
            Self::OxideEncrypted => Some("oxide"),
            Self::OxideTermJson => Some("json"),
            Self::SecureCrt => Some("xml"),
            Self::Xshell => Some("xts"),
            Self::Termius => Some("json"),
            Self::MobaXterm => Some("mxtsessions"),
            Self::WindTerm => Some("json"),
            Self::Electerm => Some("json"),
            Self::FinalShell => None,
        }
    }

    /// Whether the export target is a directory instead of one file.
    pub fn is_directory(self) -> bool {
        matches!(self, Self::FinalShell)
    }
}

/// Content produced for a single-file export. Binary formats (Xshell archive)
/// must be written as bytes; everything else is UTF-8 text.
#[derive(Debug)]
pub enum SessionExportContent {
    Text(String),
    Binary(Vec<u8>),
}

/// Exports saved connections to a single-file format. Secrets are excluded by
/// design: `SavedConnection.auth` references keychain IDs but never carries
/// secret material, so serialization is inherently password-free.
pub fn export_sessions(
    connections: &[SavedConnection],
    format: SessionExportFormat,
) -> Result<SessionExportContent> {
    let operation_id = Uuid::new_v4().to_string();
    tracing::debug!(
        target: "oxideterm::audit",
        operation_id,
        stage = "session.export.request",
        format = ?format,
        connection_count = connections.len(),
        "会话导出请求已进入导出模块，开始按目标格式生成内容"
    );
    let result = (|| -> Result<SessionExportContent> {
        match format {
        SessionExportFormat::OxideEncrypted => Err(anyhow::anyhow!(
            "encrypted .oxide export uses the dedicated OxideTerm dialog"
        )),
        SessionExportFormat::OxideTermJson => {
            export_oxide_json(connections).map(SessionExportContent::Text)
        }
        SessionExportFormat::SecureCrt => Ok(SessionExportContent::Text(export_securecrt_xml(
            connections,
        ))),
        SessionExportFormat::Xshell => {
            export_xshell_archive(connections).map(SessionExportContent::Binary)
        }
        SessionExportFormat::Termius => Ok(SessionExportContent::Text(export_termius(connections))),
        SessionExportFormat::MobaXterm => {
            Ok(SessionExportContent::Text(export_mobaxterm(connections)))
        }
        SessionExportFormat::WindTerm => {
            Ok(SessionExportContent::Text(export_windterm(connections)))
        }
        SessionExportFormat::Electerm => {
            Ok(SessionExportContent::Text(export_electerm(connections)))
        }
        SessionExportFormat::FinalShell => Err(anyhow::anyhow!(
            "FinalShell export requires a directory target"
        )),
        }
    })();
    match &result {
        Ok(_) => tracing::debug!(
            target: "oxideterm::audit",
            operation_id,
            stage = "session.export.response",
            format = ?format,
            result = "completed",
            "会话导出内容已生成"
        ),
        Err(_) => tracing::warn!(
            target: "oxideterm::audit",
            operation_id,
            stage = "session.export.response",
            format = ?format,
            result = "failed",
            failure_detail_redacted = true,
            "会话导出内容生成失败"
        ),
    }
    result
}

/// Exports saved connections into a FinalShell `conn` directory layout that
/// `connection_import.rs` can read back. Every group becomes a subdirectory
/// carrying its own `folder.json`; connections live beside it as
/// `<id>_connect_config.json`. Returns the number of written files.
pub fn export_sessions_to_finalshell_directory(
    connections: &[SavedConnection],
    target: &Path,
) -> Result<usize> {
    let operation_id = Uuid::new_v4().to_string();
    tracing::debug!(
        target: "oxideterm::audit",
        operation_id,
        stage = "session.export.directory.request",
        connection_count = connections.len(),
        "会话目录导出请求已进入导出模块"
    );
    let conn_dir = target.join("conn");
    std::fs::create_dir_all(&conn_dir).with_context(|| {
        format!(
            "failed to create FinalShell export directory {}",
            conn_dir.display()
        )
    })?;

    // Stable ids keep repeated exports from churning file names. Deriving the
    // folder/connection ids from the connection identity also makes re-import
    // deterministic without persisting any export-side state.
    let mut groups: BTreeMap<String, Vec<&SavedConnection>> = BTreeMap::new();
    for connection in connections {
        let group = connection
            .group
            .as_deref()
            .map(group_segments)
            .unwrap_or_default()
            .join("/");
        groups
            .entry(if group.is_empty() {
                "Default".to_string()
            } else {
                group
            })
            .or_default()
            .push(connection);
    }

    let mut written = 0usize;
    for (group_path, group_connections) in &groups {
        // FinalShell keeps one folder.json per group, stored inside the group
        // subdirectory with snake_case fields that the reader deserializes.
        let segments = group_path.split('/').collect::<Vec<_>>();
        let mut group_dir = conn_dir.clone();
        let mut current = String::new();
        let mut parent_id = "root".to_string();
        for (index, segment) in segments.iter().enumerate() {
            if index > 0 {
                current.push('/');
            }
            current.push_str(segment);
            group_dir = group_dir.join(safe_dir_name(segment));
            std::fs::create_dir_all(&group_dir).with_context(|| {
                format!(
                    "failed to create FinalShell group directory {}",
                    group_dir.display()
                )
            })?;
            let folder_id = stable_id(&current);
            let folder = serde_json::json!({
                "id": folder_id,
                "name": segment,
                "parent_id": parent_id,
                "delete_time": 0,
            });
            let folder_path = group_dir.join("folder.json");
            write_json_file(&folder_path, &folder)?;
            written += 1;
            parent_id = folder_id;
        }

        for connection in group_connections {
            let id = stable_id(&format!("{group_path}/{}", connection.name));
            let record = serde_json::json!({
                "name": connection.name,
                "host": connection.host,
                "port": connection.port,
                "user_name": connection.username,
                "parent_id": parent_id,
                "conection_type": 100,
                "delete_time": 0,
            });
            let path = group_dir.join(format!("{id}_connect_config.json"));
            write_json_file(&path, &record)?;
            written += 1;
        }
    }

    tracing::debug!(
        target: "oxideterm::audit",
        operation_id,
        stage = "session.export.directory.response",
        written_files = written,
        result = "completed",
        "会话目录导出已完成"
    );
    Ok(written)
}

/// Splits a stored group path ("Imported/Prod/Web") into clean segments,
/// dropping the default import prefix so third-party tools receive their own
/// folder structure.
fn group_segments(group: &str) -> Vec<&str> {
    group
        .split('/')
        .map(str::trim)
        .filter(|segment| !segment.is_empty() && !segment.eq_ignore_ascii_case("Imported"))
        .collect()
}

fn stable_id(seed: &str) -> String {
    // SHA-256 is already a workspace dependency of this crate; the export id
    // only needs to be stable within one export run.
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(seed.as_bytes());
    let mut id = String::with_capacity(16);
    for byte in &digest[..8] {
        id.push_str(&format!("{byte:02x}"));
    }
    id
}

fn write_json_file(path: &Path, value: &serde_json::Value) -> Result<()> {
    let text = serde_json::to_string_pretty(value).context("failed to serialize export record")?;
    std::fs::write(path, text.as_bytes())
        .with_context(|| format!("failed to write export file {}", path.display()))
}

fn export_oxide_json(connections: &[SavedConnection]) -> Result<String> {
    // Strip auth references entirely so keychain IDs from this device are
    // meaningless on another device and cannot leak credential metadata.
    let mut sanitized = Vec::with_capacity(connections.len());
    for connection in connections {
        let mut value = serde_json::to_value(connection)
            .context("failed to serialize connection for export")?;
        if let Some(object) = value.as_object_mut() {
            object.remove("auth");
            object.remove("proxyCommand");
        }
        sanitized.push(value);
    }
    serde_json::to_string_pretty(&serde_json::json!({
        "format": "oxideterm-sessions",
        "version": 1,
        "connections": sanitized,
    }))
    .context("failed to serialize sessions export")
}

/// SecureCRT "Export Sessions" XML. The reader accepts nested `<key>` frames
/// under a `Sessions` root; group segments become intermediate frames.
fn export_securecrt_xml(connections: &[SavedConnection]) -> String {
    let mut output =
        String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<key name=\"Sessions\">\n");
    for connection in connections {
        let segments = connection
            .group
            .as_deref()
            .map(group_segments)
            .unwrap_or_default();
        for (depth, segment) in segments.iter().enumerate() {
            output.push_str(&format!(
                "{}{}<key name=\"{}\">\n",
                "  ".repeat(depth + 2),
                "",
                xml_escape(segment)
            ));
        }
        let depth = segments.len() + 1;
        let indent = "  ".repeat(depth + 1);
        output.push_str(&format!(
            "{indent}<key name=\"{}\">\n",
            xml_escape(&connection.name)
        ));
        let value_indent = "  ".repeat(depth + 2);
        output.push_str(&format!(
            "{value_indent}<string name=\"Protocol Name\">SSH2</string>\n"
        ));
        output.push_str(&format!(
            "{value_indent}<string name=\"Hostname\">{}</string>\n",
            xml_escape(&connection.host)
        ));
        output.push_str(&format!(
            "{value_indent}<dword name=\"Ssh2 Port\">{}</dword>\n",
            connection.port
        ));
        output.push_str(&format!(
            "{value_indent}<string name=\"Username\">{}</string>\n",
            xml_escape(&connection.username)
        ));
        if let Some(key_path) = saved_key_path(connection) {
            output.push_str(&format!(
                "{value_indent}<string name=\"Identity Filename\">{}</string>\n",
                xml_escape(key_path)
            ));
        }
        if let Some(cert_path) = saved_cert_path(connection) {
            output.push_str(&format!(
                "{value_indent}<string name=\"Certificate Filename\">{}</string>\n",
                xml_escape(cert_path)
            ));
        }
        output.push_str(&format!("{indent}</key>\n"));
        for depth in (0..segments.len()).rev() {
            output.push_str(&format!("{}</key>\n", "  ".repeat(depth + 2)));
        }
    }
    output.push_str("</key>\n");
    output
}

/// Xshell `.xts` archive: a ZIP holding one `.xsh` file per connection.
/// Group segments become archive subdirectories; the reader derives group
/// paths from entry names.
fn export_xshell_archive(connections: &[SavedConnection]) -> Result<Vec<u8>> {
    let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for connection in connections {
        let segments = connection
            .group
            .as_deref()
            .map(group_segments)
            .unwrap_or_default();
        let mut entry_path = if segments.is_empty() {
            String::new()
        } else {
            format!("{}/", segments.join("/"))
        };
        entry_path.push_str(&safe_file_stem(&connection.name));
        entry_path.push_str(".xsh");

        let mut content = String::new();
        let _ = writeln!(content, "[Connection]");
        let _ = writeln!(content, "Host={}", connection.host);
        let _ = writeln!(content, "Port={}", connection.port);
        let _ = writeln!(content, "Protocol=SSH");
        let _ = writeln!(content, "Description={}", connection.name);
        let _ = writeln!(content);
        let _ = writeln!(content, "[CONNECTION_AUTHENTICATION]");
        let _ = writeln!(content, "UserName={}", connection.username);
        if let Some(key_path) = saved_key_path(connection) {
            let _ = writeln!(content);
            let _ = writeln!(content, "[CONNECTION_SSH]");
            let _ = writeln!(content, "XAgent=0");
            let _ = writeln!(content, "Auth2=PublicKey");
            let _ = writeln!(content, "UserKeyFile={}", key_path);
        }

        archive
            .start_file(entry_path, options)
            .with_context(|| format!("failed to add Xshell session {}", connection.name))?;
        archive
            .write_all(content.as_bytes())
            .context("failed to write Xshell session")?;
    }

    let cursor = archive
        .finish()
        .context("failed to finalize Xshell archive")?
        .into_inner();
    let mut buffer = Vec::new();
    std::io::Cursor::new(cursor)
        .read_to_end(&mut buffer)
        .context("failed to read Xshell archive")?;
    Ok(buffer)
}

/// Termius bookmarks export: a flat JSON array of host objects. The reader
/// accepts nested folders through `group`/`folder` keys and recurses arrays.
fn export_termius(connections: &[SavedConnection]) -> String {
    let hosts = connections
        .iter()
        .map(|connection| {
            serde_json::json!({
                "group": connection.group.as_deref().map(group_segments).unwrap_or_default().join("/"),
                "hostname": connection.host,
                "username": connection.username,
                "port": connection.port,
                "label": connection.name,
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_string_pretty(&hosts).unwrap_or_else(|_| "[]".to_string())
}

fn export_mobaxterm(connections: &[SavedConnection]) -> String {
    // Mirror the structure MobaXterm itself writes: the root Bookmarks holder
    // carries an ImgNum icon id, each sub-repository repeats it, and SSH
    // bookmarks use the `#109#0%...` version marker with empty password and
    // `-1` placeholders so MobaXterm can reload the file without defaults.
    let mut output = String::from("[Bookmarks]\nSubRep=\nImgNum=42\n\n");
    // Group each connection under its own sub-repository so the reader keeps
    // folder structure; a flat section is also accepted.
    for connection in connections {
        let group = connection
            .group
            .as_deref()
            .map(group_segments)
            .unwrap_or_default()
            .join("\\");
        let section = if group.is_empty() {
            "Bookmarks_1".to_string()
        } else {
            format!("Bookmarks_{}", stable_id(&group))
        };
        let _ = writeln!(output, "[{section}]");
        let _ = writeln!(output, "SubRep={group}");
        let _ = writeln!(output, "ImgNum=41");
        // MobaXterm SSH bookmark: name # version % host % port % user %
        // password % remaining session fields. Empty password and `-1` defaults
        // keep the record valid without embedding any secret material.
        let _ = writeln!(
            output,
            "{}=#109#0%{}%{}%{}%%-1%-1%%%%%-1%0%0%%%-1%0%0%0%",
            connection.name, connection.host, connection.port, connection.username,
        );
        let _ = writeln!(output);
    }
    output
}

/// WindTerm sessions export: a JSON array of `session` objects, matching the
/// shape the reader resolves (protocol/target/label/port/group).
fn export_windterm(connections: &[SavedConnection]) -> String {
    // The reader resolves dotted `session.*` keys from each array element, so
    // the export mirrors WindTerm's flat session record shape.
    let sessions = connections
        .iter()
        .map(|connection| {
            serde_json::json!({
                "session.protocol": "ssh",
                "session.target": format!(
                    "{}@{}",
                    connection.username, connection.host
                ),
                "session.label": connection.name,
                "session.port": connection.port,
                "session.group": connection
                    .group
                    .as_deref()
                    .map(group_segments)
                    .unwrap_or_default()
                    .join(">"),
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_string_pretty(&sessions).unwrap_or_else(|_| "[]".to_string())
}

/// Electerm bookmarks export: groups + bookmarks arrays in the shape the
/// reader's `ElectermBookmarksFile` expects.
fn export_electerm(connections: &[SavedConnection]) -> String {
    let mut groups = BTreeMap::<String, Vec<&SavedConnection>>::new();
    for connection in connections {
        let group = connection
            .group
            .as_deref()
            .map(group_segments)
            .unwrap_or_default()
            .join("/");
        groups
            .entry(if group.is_empty() {
                "Default".to_string()
            } else {
                group
            })
            .or_default()
            .push(connection);
    }

    let mut bookmark_groups = Vec::new();
    let mut bookmarks = Vec::new();
    for (group_path, group_connections) in &groups {
        let group_id = format!("g{}", stable_id(group_path));
        let mut bookmark_ids = Vec::new();
        for connection in group_connections {
            let id = format!(
                "b{}",
                stable_id(&format!("{group_path}/{}", connection.name))
            );
            bookmarks.push(serde_json::json!({
                "id": id,
                "title": connection.name,
                "host": connection.host,
                "username": connection.username,
                "port": connection.port,
                "type": "ssh",
                "enableSsh": true,
            }));
            bookmark_ids.push(id);
        }
        bookmark_groups.push(serde_json::json!({
            "id": group_id,
            "title": group_path,
            "bookmarkIds": bookmark_ids,
            "bookmarkGroupIds": [],
        }));
    }

    serde_json::to_string_pretty(&serde_json::json!({
        "bookmarkGroups": bookmark_groups,
        "bookmarks": bookmarks,
    }))
    .unwrap_or_else(|_| "{}".to_string())
}

fn saved_key_path(connection: &SavedConnection) -> Option<&str> {
    match &connection.auth {
        crate::SavedAuth::Key { key_path, .. } | crate::SavedAuth::Certificate { key_path, .. } => {
            (!key_path.trim().is_empty()).then(|| key_path.as_str())
        }
        crate::SavedAuth::ManagedKey { key_id, .. } => {
            (!key_id.trim().is_empty()).then(|| key_id.as_str())
        }
        _ => None,
    }
}

fn saved_cert_path(connection: &SavedConnection) -> Option<&str> {
    match &connection.auth {
        crate::SavedAuth::Certificate { cert_path, .. } => {
            (!cert_path.trim().is_empty()).then(|| cert_path.as_str())
        }
        _ => None,
    }
}

fn safe_file_stem(name: &str) -> String {
    let mut stem = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ' ') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    while stem.ends_with('.') {
        stem.pop();
    }
    if stem.trim().is_empty() {
        "session".to_string()
    } else {
        stem
    }
}

fn safe_dir_name(name: &str) -> String {
    let cleaned = safe_file_stem(name);
    if cleaned.is_empty() {
        "group".to_string()
    } else {
        cleaned
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Suggested file name for single-file exports.
pub fn suggested_file_name(format: SessionExportFormat) -> String {
    let stamp = Utc::now().format("%Y%m%d-%H%M%S");
    let extension = format.file_extension().unwrap_or("json");
    format!("oxideterm-sessions-{stamp}.{extension}")
}

/// Generates a fresh directory name for directory exports.
pub fn suggested_directory_name() -> String {
    format!(
        "oxideterm-fs-export-{}",
        Uuid::new_v4().simple().to_string()[..8].to_string()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConnectionImportSource, preview_connection_import, store::SavedConnection};
    use std::collections::HashSet;
    use std::path::PathBuf;

    fn sample_connection() -> SavedConnection {
        SavedConnection {
            id: "conn-1".to_string(),
            version: 1,
            name: "Web Server".to_string(),
            group: Some("Imported/Production".to_string()),
            notes: None,
            host: "web.example.com".to_string(),
            port: 2222,
            username: "deploy".to_string(),
            auth: crate::SavedAuth::Password {
                keychain_id: None,
                plaintext_password: None,
            },
            proxy_chain: Vec::new(),
            upstream_proxy: crate::SavedUpstreamProxyPolicy::UseGlobal,
            proxy_command: None,
            options: Default::default(),
            created_at: chrono::Utc::now(),
            last_used_at: None,
            updated_at: None,
            color: None,
            icon_background_color: None,
            icon: None,
            tags: Vec::new(),
            post_connect_command: None,
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("oxideterm-session-export-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    fn roundtrip(format: SessionExportFormat, file_name: &str) {
        let connections = vec![sample_connection()];
        let content = export_sessions(&connections, format).expect("export");
        let path = temp_path(file_name);
        match &content {
            SessionExportContent::Text(text) => std::fs::write(&path, text.as_bytes()).unwrap(),
            SessionExportContent::Binary(bytes) => std::fs::write(&path, bytes).unwrap(),
        }
        let preview = preview_connection_import(
            ConnectionImportSource::from_export(format),
            &[path.display().to_string()],
            &HashSet::new(),
        )
        .expect("re-import exported file");
        assert_eq!(preview.total, 1, "roundtrip lost sessions for {format:?}");
        let draft = &preview.drafts[0];
        assert_eq!(
            draft.host, "web.example.com",
            "host mismatch for {format:?}"
        );
        assert_eq!(draft.port, 2222, "port mismatch for {format:?}");
        assert_eq!(draft.username, "deploy", "username mismatch for {format:?}");
        assert!(
            draft
                .group
                .as_deref()
                .is_some_and(|group| group.contains("Production")),
            "group not preserved for {format:?}: {:?}",
            draft.group
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    impl ConnectionImportSource {
        fn from_export(format: SessionExportFormat) -> Self {
            match format {
                SessionExportFormat::OxideEncrypted | SessionExportFormat::OxideTermJson => {
                    panic!("native format is not a third-party client")
                }
                SessionExportFormat::SecureCrt => Self::SecureCrt,
                SessionExportFormat::Xshell => Self::Xshell,
                SessionExportFormat::Termius => Self::Termius,
                SessionExportFormat::MobaXterm => Self::MobaXterm,
                SessionExportFormat::WindTerm => Self::WindTerm,
                SessionExportFormat::Electerm => Self::Electerm,
                SessionExportFormat::FinalShell => Self::FinalShell,
            }
        }
    }

    #[test]
    fn securecrt_xml_roundtrips() {
        roundtrip(SessionExportFormat::SecureCrt, "sessions.xml");
    }

    #[test]
    fn xshell_archive_roundtrips() {
        roundtrip(SessionExportFormat::Xshell, "sessions.xts");
    }

    #[test]
    fn termius_json_roundtrips() {
        roundtrip(SessionExportFormat::Termius, "termius.json");
    }

    #[test]
    fn mobaxterm_roundtrips() {
        roundtrip(SessionExportFormat::MobaXterm, "sessions.mxtsessions");
    }

    #[test]
    fn windterm_json_roundtrips() {
        roundtrip(SessionExportFormat::WindTerm, "windterm.json");
    }

    #[test]
    fn electerm_json_roundtrips() {
        roundtrip(SessionExportFormat::Electerm, "electerm.json");
    }

    #[test]
    fn finalshell_directory_roundtrips() {
        let connections = vec![sample_connection()];
        let dir = std::env::temp_dir().join(format!("oxideterm-fs-export-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let written = export_sessions_to_finalshell_directory(&connections, &dir).expect("export");
        assert!(
            written >= 2,
            "expected folder + connection files, got {written}"
        );
        let preview = preview_connection_import(
            ConnectionImportSource::FinalShell,
            &[dir.display().to_string()],
            &HashSet::new(),
        )
        .expect("re-import FinalShell export");
        assert_eq!(preview.total, 1);
        let draft = &preview.drafts[0];
        assert_eq!(draft.host, "web.example.com");
        assert!(
            draft
                .group
                .as_deref()
                .is_some_and(|group| group.contains("Production"))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
