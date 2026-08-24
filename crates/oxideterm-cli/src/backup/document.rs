// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use oxideterm_connections::{ConnectionStore, SavedConnectionsSyncSnapshot};
use oxideterm_settings::export_oxide_settings_snapshot_json;
use serde::Serialize;
use serde_json::Value;

use crate::{
    error::{CliError, CliResult, runtime_error},
    paths::{self, default_backups_dir, default_connections_path},
    settings,
};

pub(super) const BACKUP_FORMAT: &str = "oxideterm-cli-backup-v1";
const BACKUP_FILE_PREFIX: &str = "oxideterm-backup-";
const BACKUP_FILE_EXTENSION: &str = "json";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BackupDocument {
    pub(super) format: &'static str,
    pub(super) created_at_ms: u64,
    pub(super) source_paths: paths::CliPaths,
    pub(super) settings: Value,
    pub(super) connections: SavedConnectionsSyncSnapshot,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BackupSummary {
    pub(super) format: &'static str,
    pub(super) settings_section_count: usize,
    pub(super) connection_record_count: usize,
}

pub(super) fn build_backup_document(json: bool) -> CliResult<BackupDocument> {
    let created_at_ms = now_ms();
    let settings = settings::load_settings_read_only(json)?;
    let settings_snapshot_json =
        export_oxide_settings_snapshot_json(&settings.settings, None, false)
            .map_err(|error| CliError::new("settings_export_failed", error.to_string(), json))?;
    let settings_snapshot = serde_json::from_str::<Value>(&settings_snapshot_json)
        .map_err(|error| CliError::new("serialization_failed", error.to_string(), json))?;

    let connections_store = ConnectionStore::load_read_only(default_connections_path())
        .map_err(|error| runtime_error(error, json))?;
    let connections = connections_store
        .export_saved_connections_snapshot()
        .map_err(|error| runtime_error(error, json))?;

    Ok(BackupDocument {
        format: BACKUP_FORMAT,
        created_at_ms,
        source_paths: paths::cli_paths(),
        settings: settings_snapshot,
        connections,
    })
}

pub(super) fn backup_summary_from_document(backup: &BackupDocument) -> BackupSummary {
    BackupSummary {
        format: backup.format,
        settings_section_count: backup
            .settings
            .get("sectionIds")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or_default(),
        connection_record_count: backup.connections.records.len(),
    }
}

pub(super) fn backup_file_name(created_at_ms: u64) -> String {
    format!("{BACKUP_FILE_PREFIX}{created_at_ms}.{BACKUP_FILE_EXTENSION}")
}

pub(super) fn is_backup_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.starts_with(BACKUP_FILE_PREFIX)
                && path.extension().and_then(|ext| ext.to_str()) == Some(BACKUP_FILE_EXTENSION)
        })
}

pub(super) fn resolve_backup_query(query: &str) -> PathBuf {
    let path = PathBuf::from(query);
    if path.is_absolute() || path.components().count() > 1 {
        return path;
    }
    let file_name = if path.extension().is_some() {
        query.to_string()
    } else {
        format!("{query}.{BACKUP_FILE_EXTENSION}")
    };
    default_backups_dir().join(file_name)
}

pub(super) fn read_backup_value(path: &Path, json: bool) -> CliResult<Value> {
    let contents = fs::read_to_string(path).map_err(|error| {
        CliError::new(
            "backup_read_failed",
            format!("failed to read backup {}: {error}", path.display()),
            json,
        )
    })?;
    serde_json::from_str::<Value>(&contents).map_err(|error| {
        CliError::new(
            "backup_parse_failed",
            format!("failed to parse backup {}: {error}", path.display()),
            json,
        )
    })
}

pub(super) fn format_backup_summary(backup: &Value) -> String {
    let created_at = backup
        .get("createdAtMs")
        .and_then(Value::as_u64)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string());
    let connection_count = backup
        .get("connections")
        .and_then(|connections| connections.get("records"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    let settings_sections = backup
        .get("settings")
        .and_then(|settings| settings.get("sectionIds"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    format!(
        "format: {}\ncreatedAtMs: {}\nsettingsSections: {}\nconnections: {}",
        backup
            .get("format")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
        created_at,
        settings_sections,
        connection_count
    )
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_names_are_recognized_by_prefix_and_extension() {
        let path = PathBuf::from("oxideterm-backup-123.json");

        assert!(is_backup_file(&path));
        assert!(!is_backup_file(Path::new("other.json")));
    }

    #[test]
    fn inspect_query_resolves_plain_file_names_under_backup_dir() {
        let path = resolve_backup_query("oxideterm-backup-123");

        assert!(path.ends_with("oxideterm-backup-123.json"));
    }
}
