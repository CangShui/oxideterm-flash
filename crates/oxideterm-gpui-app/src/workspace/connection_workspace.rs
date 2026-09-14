use std::{
    collections::{HashMap, HashSet, VecDeque, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use crate::workspace::new_connection::{
    NewConnectionProxyHop, NewConnectionUpstreamProxyAuth, NewConnectionUpstreamProxyPolicy,
    identity_agent_from_form, identity_agent_selector, ssh_auth_tab_from_saved_auth,
};
use crate::workspace::quick_commands::QuickCommandImportStrategy;
use chrono::{Local, Utc};
use gpui::{Div, EventEmitter, Task, prelude::*, rgba};
use oxideterm_connections::{
    ConnectionAuthDraft, ConnectionAuthDraftKind, ConnectionDraft,
    ConnectionStore, ProxyHopDraft, SaveConnectionRequest,
    SavedAuth, SavedConnection, SavedProxyCommand, SavedUpstreamProxyAuth,
    SavedUpstreamProxyConfig, SavedUpstreamProxyPolicy, SavedUpstreamProxyProtocol, SecretString,
    oxide_file::{
        ExportPreflightResult, ForwardDetail, ImportConflictStrategy, ImportPreview,
        ImportResultEnvelope, OxideExportOptions, OxideFile, OxideFileError, OxideForwardRecord,
        OxideImportOptions, OxideMetadata, apply_oxide_import_with_options_with_progress,
        export_connections_to_oxide_with_progress, preflight_export,
        preview_oxide_import_with_progress,
    },
    save_request_from_draft, validate_group_name,
};
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_forwarding::{ForwardType, OwnedForwardImportRecord, PersistedForward};
use oxideterm_gpui_ui::{
    button::{ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions, ToolbarButtonOptions},
    checkbox,
    modal::dismissible_dialog_backdrop,
    surface::{color_for_background, color_for_background_or_alpha},
    text_input::{
        text_caret, text_input_anchor_probe, text_input_secret_mask, text_input_value_segments,
        text_input_visual_range,
    },
};
use oxideterm_session_adapter::upstream_proxy_config_from_saved_policy;
use oxideterm_settings::{
    ALL_OXIDE_SETTINGS_SECTIONS, DEFAULT_OXIDE_SETTINGS_SECTIONS, PersistedSettings,
    export_oxide_settings_snapshot_json, merge_oxide_settings_snapshot,
};
use oxideterm_ssh::{UpstreamProxyAuth, UpstreamProxyConfig, UpstreamProxyProtocol};
use zeroize::Zeroizing;

use super::*;
use crate::workspace::ime::WorkspaceImeTarget;

const BG_ACTIVE_THEME_ALPHA: u32 = 0x66; // Tauri [data-bg-active] color-mix(... 40%, transparent)
const BG_ACTIVE_HOVER_ALPHA: u32 = 0x80; // Tauri bg-hover 50%
const BG_ACTIVE_ROW_HOVER_ALPHA: u32 = 0x4d; // Keep full-width row hover quieter than compact controls.
const ROW_HOVER_ALPHA: u32 = 0x66; // Plain-theme rows use the same restrained hierarchy as image-backed rows.
const BG_ACTIVE_BORDER_ALPHA: u32 = 0xbf; // Tauri border 75%
const BG_ACTIVE_BORDER_HALF_ALPHA: u32 = 0x60; // Tauri border/50 after active border mix
#[allow(dead_code)]
const MANAGER_GRID_ESTIMATED_ROW_HEIGHT: f32 = 84.0;
#[allow(dead_code)]
const MANAGER_LIST_ESTIMATED_ROW_HEIGHT: f32 = 57.0;
#[allow(dead_code)]
const MANAGER_TREE_ESTIMATED_ROW_HEIGHT: f32 = 52.0;
#[allow(dead_code)]
const MANAGER_MAIN_VIEW_OVERSCAN: usize = 6;
const OXIDE_APP_SETTINGS_SECTIONS: &[&str] = ALL_OXIDE_SETTINGS_SECTIONS;
const OXIDE_MODAL_WIDTH: f32 = 672.0; // Tauri max-w-2xl
const OXIDE_MODAL_MAX_HEIGHT_RATIO: f32 = 0.85; // Tauri max-h-[85vh]
const OXIDE_MODAL_HEADER_PX: f32 = 24.0; // Tauri px-6
const OXIDE_MODAL_HEADER_PY: f32 = 16.0; // Tauri py-4
const OXIDE_MODAL_BODY_P: f32 = 24.0; // Tauri p-6
const OXIDE_MODAL_SECTION_GAP: f32 = 16.0; // Tauri space-y-4
const OXIDE_MODAL_CARD_P: f32 = 12.0; // Tauri p-3
const OXIDE_MODAL_LIST_MAX_H: f32 = 256.0; // Tauri max-h-64
const OXIDE_MODAL_FORWARDS_MAX_H: f32 = 208.0; // Tauri max-h-52
const OXIDE_SELECT_ALL_BUTTON_HEIGHT: f32 = 28.0; // Tauri OxideExportModal Button h-7
const OXIDE_BLUE_500: u32 = 0x3b82f6;
const OXIDE_GREEN_500: u32 = 0x22c55e;
const OXIDE_YELLOW_500: u32 = 0xeab308;
const OXIDE_RED_500: u32 = 0xef4444;
const OXIDE_ORANGE_500: u32 = 0xf97316;
const OXIDE_SLATE_400: u32 = 0x94a3b8;
const OXIDE_TONE_BG_ALPHA: u32 = 0x1a; // Tauri *-500/10
const OXIDE_TONE_BORDER_ALPHA: u32 = 0x33; // Tauri *-500/20
const OXIDE_SUBCARD_BG_ALPHA: u32 = 0x99; // Tauri bg-theme-bg-elevated/60 and bg-theme-bg/60
const OXIDE_NEW_BADGE_BG_ALPHA: u32 = 0x26; // Tauri bg-green-500/15

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum ConnectionWorkspaceInput {
    OxideImportPassword,
    OxideExportPassword,
    OxideExportConfirmPassword,
    OxideExportDescription,
}

impl ConnectionWorkspaceInput {
    pub(super) fn anchor_key(self) -> u64 {
        match self {
            Self::OxideImportPassword => 4,
            Self::OxideExportPassword => 5,
            Self::OxideExportConfirmPassword => 6,
            Self::OxideExportDescription => 7,
        }
    }

    pub(super) fn is_secret(self) -> bool {
        matches!(
            self,
            Self::OxideImportPassword
                | Self::OxideExportPassword
                | Self::OxideExportConfirmPassword
        )
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct OxideImportResultView {
    pub(super) imported: usize,
    pub(super) skipped: usize,
    pub(super) merged: usize,
    pub(super) replaced: usize,
    pub(super) renamed: usize,
    pub(super) renames: Vec<(String, String)>,
    pub(super) errors: Vec<String>,
    pub(super) imported_forwards: usize,
    pub(super) skipped_forwards: usize,
    pub(super) imported_app_settings: bool,
    pub(super) skipped_app_settings: bool,
    pub(super) imported_quick_commands: usize,
    pub(super) skipped_quick_commands: bool,
    pub(super) imported_serial_profiles: usize,
    pub(super) skipped_serial_profiles: usize,
    pub(super) imported_telnet_profiles: usize,
    pub(super) skipped_telnet_profiles: usize,
    pub(super) quick_commands_errors: Vec<String>,
    pub(super) imported_plugin_settings: usize,
    pub(super) skipped_plugin_settings: bool,
    pub(super) imported_portable_secrets: usize,
    pub(super) skipped_portable_secrets: usize,
}

#[derive(Clone, Debug)]
pub(super) struct OxideTransferProgress {
    pub(super) stage: String,
    pub(super) current: usize,
    pub(super) total: usize,
}

impl OxideTransferProgress {
    pub(super) fn new(stage: impl Into<String>, current: usize, total: usize) -> Self {
        Self {
            stage: stage.into(),
            current,
            total,
        }
    }

    pub(super) fn percent(&self) -> usize {
        if self.total == 0 {
            0
        } else {
            ((self.current.min(self.total) * 100) / self.total).min(100)
        }
    }
}

pub(super) struct ConnectionWorkspaceState {
    pub(super) focused_input: Option<ConnectionWorkspaceInput>,
    pub(super) oxide_import_dialog: Option<OxideImportDialogState>,
    pub(super) oxide_export_dialog: Option<OxideExportDialogState>,
    pub(super) status: Option<String>,
    pub(super) oxide_export_connection_list_state: ListState,
    pub(super) oxide_export_connection_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) oxide_import_connection_preview_list_state: ListState,
    pub(super) oxide_import_connection_preview_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) oxide_export_forward_group_list_state: ListState,
    pub(super) oxide_export_forward_group_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) oxide_export_summary_line_list_state: ListState,
    pub(super) oxide_export_summary_line_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) oxide_import_forward_detail_list_state: ListState,
    pub(super) oxide_import_forward_detail_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) oxide_import_name_group_list_states: RefCell<HashMap<String, ListState>>,
    pub(super) oxide_import_name_group_list_caches:
        RefCell<HashMap<String, VirtualListSignatureCache>>,
    pub(super) import_dialog_exit_task: Option<Task<()>>,
    pub(super) export_dialog_exit_task: Option<Task<()>>,
    pub(super) dialog_auto_close_task: Option<Task<()>>,
    pub(super) import_file_picker_task: Option<Task<()>>,
    pub(super) export_file_picker_task: Option<Task<()>>,
    oxide_worker_tx: Option<delivery::ActiveDeliverySender<oxide_actions::OxideWorkerDelivery>>,
    oxide_worker_rx: Option<std::sync::mpsc::Receiver<oxide_actions::OxideWorkerDelivery>>,
    _oxide_delivery_task: Option<Task<()>>,
    oxide_worker_threads: HashMap<oxide_actions::OxideWorkerKey, std::thread::JoinHandle<()>>,
}

impl Default for ConnectionWorkspaceState {
    fn default() -> Self {
        Self {
            focused_input: None,
            oxide_import_dialog: None,
            oxide_export_dialog: None,
            status: None,
            oxide_export_connection_list_state: ListState::new(
                OXIDE_EXPORT_CONNECTION_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(OXIDE_EXPORT_CONNECTION_LIST_ESTIMATED_HEIGHT),
                    OXIDE_EXPORT_CONNECTION_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            oxide_export_connection_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            oxide_import_connection_preview_list_state: ListState::new(
                OXIDE_IMPORT_CONNECTION_PREVIEW_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(OXIDE_IMPORT_CONNECTION_PREVIEW_LIST_ESTIMATED_HEIGHT),
                    OXIDE_IMPORT_CONNECTION_PREVIEW_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            oxide_import_connection_preview_list_cache: RefCell::new(
                VirtualListSignatureCache::default(),
            ),
            oxide_export_forward_group_list_state: ListState::new(
                OXIDE_EXPORT_FORWARD_GROUP_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(OXIDE_EXPORT_FORWARD_GROUP_LIST_ESTIMATED_HEIGHT),
                    OXIDE_EXPORT_FORWARD_GROUP_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            oxide_export_forward_group_list_cache: RefCell::new(
                VirtualListSignatureCache::default(),
            ),
            oxide_export_summary_line_list_state: ListState::new(
                OXIDE_EXPORT_SUMMARY_LINE_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(OXIDE_EXPORT_SUMMARY_LINE_LIST_ESTIMATED_HEIGHT),
                    OXIDE_EXPORT_SUMMARY_LINE_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            oxide_export_summary_line_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            oxide_import_forward_detail_list_state: ListState::new(
                OXIDE_IMPORT_FORWARD_DETAIL_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(OXIDE_IMPORT_FORWARD_DETAIL_LIST_ESTIMATED_HEIGHT),
                    OXIDE_IMPORT_FORWARD_DETAIL_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            oxide_import_forward_detail_list_cache: RefCell::new(
                VirtualListSignatureCache::default(),
            ),
            oxide_import_name_group_list_states: RefCell::new(HashMap::new()),
            oxide_import_name_group_list_caches: RefCell::new(HashMap::new()),
            import_dialog_exit_task: None,
            export_dialog_exit_task: None,
            dialog_auto_close_task: None,
            import_file_picker_task: None,
            export_file_picker_task: None,
            oxide_worker_tx: None,
            oxide_worker_rx: None,
            _oxide_delivery_task: None,
            oxide_worker_threads: HashMap::new(),
        }
    }
}

impl EventEmitter<ConnectionWorkspaceEvent> for ConnectionWorkspaceState {}

pub(super) enum ConnectionWorkspaceEvent {
    OxideEffectsReady(oxide_actions::OxideWorkspaceEffects),
    RefreshOxideExportPreflight,
}

impl ConnectionWorkspaceState {
    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        let mut state = Self::default();
        state.initialize_oxide_delivery(cx);
        state
    }

    pub(in crate::workspace) fn focused_input(&self) -> Option<ConnectionWorkspaceInput> {
        self.focused_input
    }

    pub(in crate::workspace) fn clear_input_focus(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self.focused_input.take().is_some();
        if changed {
            cx.notify();
        }
        changed
    }

    pub(in crate::workspace) fn input_value(&self, input: ConnectionWorkspaceInput) -> Option<&str> {
        match input {
            ConnectionWorkspaceInput::OxideImportPassword => self
                .oxide_import_dialog
                .as_ref()
                .map(|dialog| dialog.password.as_str()),
            ConnectionWorkspaceInput::OxideExportPassword => self
                .oxide_export_dialog
                .as_ref()
                .map(|dialog| dialog.password.as_str()),
            ConnectionWorkspaceInput::OxideExportConfirmPassword => self
                .oxide_export_dialog
                .as_ref()
                .map(|dialog| dialog.confirm_password.as_str()),
            ConnectionWorkspaceInput::OxideExportDescription => self
                .oxide_export_dialog
                .as_ref()
                .map(|dialog| dialog.description.as_str()),
        }
    }

    pub(in crate::workspace) fn replace_input(
        &mut self,
        input: ConnectionWorkspaceInput,
        replacement_range: Option<std::ops::Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let value = match input {
            ConnectionWorkspaceInput::OxideImportPassword => {
                let Some(dialog) = self.oxide_import_dialog.as_mut() else {
                    return false;
                };
                dialog.error = None;
                &mut dialog.password
            }
            ConnectionWorkspaceInput::OxideExportPassword => {
                let Some(dialog) = self.oxide_export_dialog.as_mut() else {
                    return false;
                };
                dialog.error = None;
                &mut dialog.password
            }
            ConnectionWorkspaceInput::OxideExportConfirmPassword => {
                let Some(dialog) = self.oxide_export_dialog.as_mut() else {
                    return false;
                };
                dialog.error = None;
                &mut dialog.confirm_password
            }
            ConnectionWorkspaceInput::OxideExportDescription => {
                let Some(dialog) = self.oxide_export_dialog.as_mut() else {
                    return false;
                };
                dialog.error = None;
                &mut dialog.description
            }
        };
        replace_utf16(value, replacement_range, text);
        cx.notify();
        true
    }
}

pub(super) struct OxideImportDialogState {
    pub(super) presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(super) file_path: Option<PathBuf>,
    pub(super) file_data: Option<Arc<[u8]>>,
    pub(super) metadata_summary: Option<String>,
    pub(super) metadata: Option<OxideMetadata>,
    pub(super) password: Zeroizing<String>,
    pub(super) conflict_strategy: ImportConflictStrategy,
    pub(super) preview: Option<Arc<ImportPreview>>,
    pub(super) selected_names: HashSet<String>,
    pub(super) import_app_settings: bool,
    pub(super) selected_app_settings_sections: HashSet<String>,
    pub(super) expanded_app_settings_sections: HashSet<String>,
    pub(super) import_quick_commands: bool,
    pub(super) import_serial_profiles: bool,
    pub(super) import_telnet_profiles: bool,
    pub(super) import_plugin_settings: bool,
    pub(super) selected_plugin_ids: HashSet<String>,
    pub(super) import_forwards: bool,
    pub(super) import_portable_secrets: bool,
    pub(super) restore_managed_keys: bool,
    pub(super) restore_managed_key_passphrases: bool,
    pub(super) busy: bool,
    pub(super) operation_generation: u64,
    pub(super) progress_stage: Option<OxideTransferProgress>,
    pub(super) focused_footer_action: Option<OxideDialogFooterAction>,
    pub(super) error: Option<String>,
    pub(super) result_summary: Option<String>,
    pub(super) result: Option<Arc<OxideImportResultView>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OxideDialogFooterAction {
    Cancel,
    Secondary,
    Primary,
}

impl Default for OxideImportDialogState {
    fn default() -> Self {
        Self {
            presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            file_path: None,
            file_data: None,
            metadata_summary: None,
            metadata: None,
            password: Zeroizing::new(String::new()),
            conflict_strategy: ImportConflictStrategy::Rename,
            preview: None,
            selected_names: HashSet::new(),
            import_app_settings: true,
            selected_app_settings_sections: OXIDE_APP_SETTINGS_SECTIONS
                .iter()
                .map(|section| (*section).to_string())
                .collect(),
            expanded_app_settings_sections: HashSet::new(),
            import_quick_commands: true,
            import_serial_profiles: true,
            import_telnet_profiles: true,
            import_plugin_settings: true,
            selected_plugin_ids: HashSet::new(),
            import_forwards: true,
            import_portable_secrets: false,
            restore_managed_keys: true,
            restore_managed_key_passphrases: false,
            busy: false,
            operation_generation: 0,
            progress_stage: None,
            focused_footer_action: Some(OxideDialogFooterAction::Secondary),
            error: None,
            result_summary: None,
            result: None,
        }
    }
}

impl std::fmt::Debug for OxideImportDialogState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OxideImportDialogState")
            .field("file_path", &self.file_path)
            .field("file_data", &self.file_data.as_ref().map(|data| data.len()))
            .field("metadata_summary", &self.metadata_summary)
            .field("metadata", &self.metadata)
            .field("password", &"[redacted secret]")
            .field("conflict_strategy", &self.conflict_strategy)
            .field("preview", &self.preview)
            .field("selected_names", &self.selected_names)
            .field("import_app_settings", &self.import_app_settings)
            .field(
                "selected_app_settings_sections",
                &self.selected_app_settings_sections,
            )
            .field(
                "expanded_app_settings_sections",
                &self.expanded_app_settings_sections,
            )
            .field("import_quick_commands", &self.import_quick_commands)
            .field("import_serial_profiles", &self.import_serial_profiles)
            .field("import_telnet_profiles", &self.import_telnet_profiles)
            .field("import_plugin_settings", &self.import_plugin_settings)
            .field("selected_plugin_ids", &self.selected_plugin_ids)
            .field("import_forwards", &self.import_forwards)
            .field("import_portable_secrets", &self.import_portable_secrets)
            .field("restore_managed_keys", &self.restore_managed_keys)
            .field(
                "restore_managed_key_passphrases",
                &self.restore_managed_key_passphrases,
            )
            .field("busy", &self.busy)
            .field("operation_generation", &self.operation_generation)
            .field("progress_stage", &self.progress_stage)
            .field("focused_footer_action", &self.focused_footer_action)
            .field("error", &self.error)
            .field("result_summary", &self.result_summary)
            .field("result", &self.result)
            .finish()
    }
}

pub(super) struct OxideExportDialogState {
    pub(super) presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(super) selected_ids: HashSet<String>,
    connection_rows: Arc<[oxide_export_selection_dialogs::OxideExportConnectionRow]>,
    forward_group_rows: Arc<[oxide_export_selection_dialogs::OxideExportForwardGroupRow]>,
    pub(super) available_forwards: Vec<PersistedForward>,
    pub(super) selected_forward_ids: HashSet<String>,
    pub(super) include_app_settings: bool,
    pub(super) selected_app_settings_sections: HashSet<String>,
    pub(super) include_local_terminal_env_vars: bool,
    pub(super) include_quick_commands: bool,
    pub(super) include_serial_profiles: bool,
    pub(super) include_telnet_profiles: bool,
    pub(super) include_remote_desktop_profiles: bool,
    pub(super) include_plugin_settings: bool,
    pub(super) plugin_groups: HashMap<String, usize>,
    pub(super) selected_plugin_ids: HashSet<String>,
    pub(super) include_forwards: bool,
    pub(super) include_portable_secrets: bool,
    pub(super) embed_keys: bool,
    pub(super) include_passwords: bool,
    pub(super) include_key_passphrases: bool,
    pub(super) include_managed_keys: bool,
    pub(super) include_managed_key_passphrases: bool,
    pub(super) password: Zeroizing<String>,
    pub(super) confirm_password: Zeroizing<String>,
    pub(super) description: String,
    pub(super) busy: bool,
    pub(super) operation_generation: u64,
    pub(super) progress_stage: Option<OxideTransferProgress>,
    pub(super) focused_footer_action: Option<OxideDialogFooterAction>,
    pub(super) last_export_timestamp: Option<i64>,
    pub(super) preflight: Option<ExportPreflightResult>,
    pub(super) error: Option<String>,
    pub(super) result_summary: Option<String>,
}

impl Default for OxideExportDialogState {
    fn default() -> Self {
        Self {
            presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            selected_ids: HashSet::new(),
            connection_rows: Arc::from([]),
            forward_group_rows: Arc::from([]),
            available_forwards: Vec::new(),
            selected_forward_ids: HashSet::new(),
            include_app_settings: true,
            selected_app_settings_sections: DEFAULT_OXIDE_SETTINGS_SECTIONS
                .iter()
                .map(|section| (*section).to_string())
                .collect(),
            include_local_terminal_env_vars: false,
            include_quick_commands: true,
            include_serial_profiles: true,
            include_telnet_profiles: true,
            include_remote_desktop_profiles: true,
            include_plugin_settings: true,
            plugin_groups: HashMap::new(),
            selected_plugin_ids: HashSet::new(),
            include_forwards: true,
            include_portable_secrets: false,
            embed_keys: false,
            include_passwords: false,
            include_key_passphrases: true,
            include_managed_keys: true,
            include_managed_key_passphrases: false,
            password: Zeroizing::new(String::new()),
            confirm_password: Zeroizing::new(String::new()),
            description: String::new(),
            busy: false,
            operation_generation: 0,
            progress_stage: None,
            focused_footer_action: Some(OxideDialogFooterAction::Cancel),
            last_export_timestamp: None,
            preflight: None,
            error: None,
            result_summary: None,
        }
    }
}

impl std::fmt::Debug for OxideExportDialogState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OxideExportDialogState")
            .field("selected_ids", &self.selected_ids)
            .field("available_forwards", &self.available_forwards)
            .field("selected_forward_ids", &self.selected_forward_ids)
            .field("include_app_settings", &self.include_app_settings)
            .field(
                "selected_app_settings_sections",
                &self.selected_app_settings_sections,
            )
            .field(
                "include_local_terminal_env_vars",
                &self.include_local_terminal_env_vars,
            )
            .field("include_quick_commands", &self.include_quick_commands)
            .field("include_serial_profiles", &self.include_serial_profiles)
            .field("include_telnet_profiles", &self.include_telnet_profiles)
            .field(
                "include_remote_desktop_profiles",
                &self.include_remote_desktop_profiles,
            )
            .field("include_plugin_settings", &self.include_plugin_settings)
            .field("plugin_groups", &self.plugin_groups)
            .field("selected_plugin_ids", &self.selected_plugin_ids)
            .field("include_forwards", &self.include_forwards)
            .field("include_portable_secrets", &self.include_portable_secrets)
            .field("embed_keys", &self.embed_keys)
            .field("include_passwords", &self.include_passwords)
            .field("include_key_passphrases", &self.include_key_passphrases)
            .field("include_managed_keys", &self.include_managed_keys)
            .field(
                "include_managed_key_passphrases",
                &self.include_managed_key_passphrases,
            )
            .field("password", &"[redacted secret]")
            .field("confirm_password", &"[redacted secret]")
            .field("description", &self.description)
            .field("busy", &self.busy)
            .field("operation_generation", &self.operation_generation)
            .field("progress_stage", &self.progress_stage)
            .field("focused_footer_action", &self.focused_footer_action)
            .field("last_export_timestamp", &self.last_export_timestamp)
            .field("preflight", &self.preflight)
            .field("error", &self.error)
            .field("result_summary", &self.result_summary)
            .finish()
    }
}

// Keep the manager split by UI surface and behavior while preserving one workspace boundary.
mod actions;
mod helpers;
mod oxide_actions;
mod oxide_dialog_common;
mod oxide_dialog_helpers;
mod oxide_export_dialogs;
mod oxide_export_selection_dialogs;
mod oxide_export_summary_dialogs;
mod oxide_import_dialogs;
mod oxide_import_preview_dialogs;
mod oxide_import_result_dialogs;
mod oxide_inputs;

// Recreate the former flat include scope without exposing internal helpers to the workspace.
#[allow(unused_imports)]
use self::{
    actions::*, helpers::*, oxide_actions::*, oxide_dialog_common::*, oxide_dialog_helpers::*,
    oxide_export_dialogs::*, oxide_export_selection_dialogs::*, oxide_export_summary_dialogs::*,
    oxide_import_dialogs::*, oxide_import_preview_dialogs::*, oxide_import_result_dialogs::*,
    oxide_inputs::*,
};

// Preserve the workspace-facing session manager API at its original visibility.
#[cfg(test)]
pub(in crate::workspace) use self::helpers::save_request_from_form;
pub(in crate::workspace) use self::helpers::{
    RuntimeSecretHandoff, duplicate_connection_template_name, form_from_saved_connection,
    restore_legacy_jump_host_in_form, save_request_from_form_with_existing_auth,
    save_request_from_form_with_proxy_hop_prefix, upstream_proxy_config_from_form,
};

#[cfg(test)]
mod tests;
