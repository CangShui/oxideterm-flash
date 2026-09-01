use std::sync::atomic::{AtomicBool, Ordering};

use gpui::{
    AnchoredPositionMode, Corner, Div, ObjectFit, PathPromptOptions, Rgba, anchored, deferred, img,
    point, relative,
};
use oxideterm_settings::{
    AppIconVariant, CloudSyncMode, FrostedGlassMode, HighlightRule, HighlightRuleSet, Language,
    MAX_HIGHLIGHT_RULE_SETS, MAX_HIGHLIGHT_RULES, PersistedSettings,
    RECOMMENDED_FOCUS_HANDOFF_COMMANDS, SettingsApplicationProxyMode,
    SettingsUpstreamProxyAuth, SettingsUpstreamProxyConfig, SettingsUpstreamProxyProtocol,
    TerminalSemanticScheme, UpdateChannel, UpdateProxyMode, UpdateProxyProtocol,
    create_default_highlight_rule, reindex_highlight_rules, sanitize_highlight_rule_sets,
};
use oxideterm_settings_model::{
    CUSTOM_SEMANTIC_SCHEME_PREFIX, MAX_SEMANTIC_RULES, SEMANTIC_CLASSES,
    SETTINGS_SECTION_HEADER_ITEM_COUNT, SemanticClass, SemanticRuleContext,
    SemanticRuleDefinition, SemanticSchemeDocument, SettingsDynamicSectionCounts,
    SettingsInputDraftApply, add_custom_semantic_rule, apply_persisted_settings_input_draft,
    create_custom_semantic_scheme, delete_custom_semantic_rule, delete_custom_semantic_scheme,
    edit_custom_semantic_scheme, export_custom_semantic_scheme, import_custom_semantic_scheme_named,
    persisted_settings_input_value, reconnect_base_delay_options,
    reconnect_max_attempt_options,
    reconnect_max_delay_options, settings_multiline_line_ranges, settings_multiline_line_selection,
    settings_section_list_identity as settings_model_section_list_identity,
    settings_section_list_item_count as settings_model_section_list_item_count,
};
use oxideterm_ssh::{HostKeyStatus, UpstreamProxyConfig, probe_upstream_proxy_route};

use super::*;
use super::ime::WorkspaceImeTarget;
use oxideterm_connections::{
    ConnectionImportApplyRequest, ConnectionImportDuplicateStrategy, ConnectionImportPreview,
    ConnectionImportSource, ImportedConnectionAuthType,
    ManagedSshKeyInfo, ManagedSshKeyOrigin, ManagedSshKeyUsage,
    SecretString, SshConfigHost, apply_connection_import, list_available_ssh_keys,
    list_ssh_config_hosts, preview_connection_import,
};
use oxideterm_gpui_platform::vibrancy::{NativeVibrancyMode, VibrancySupport, available_modes};
use oxideterm_gpui_settings_view::*;
use oxideterm_gpui_ui::{
    ConfirmDialogVariant, ConfirmDialogView,
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        SplitFooterButtonEdge, SplitFooterButtonOptions, ToolbarButtonIconPosition,
        ToolbarButtonOptions, split_footer_button,
    },
    checkbox,
    entity_row::{EntityListRowOptions, entity_list_row},
    modal::{
        dialog_content, dialog_description, dialog_footer, dialog_header, dialog_title,
        dismissible_dialog_backdrop, overlay_content_boundary, popover_backdrop,
    },
    select::{
        OverlayAnchor, SelectAnchorId, select_anchor_probe, select_label, select_option,
        select_option_action, select_overlay_popup, select_panel_overlay_popup_with_max_height,
        select_separator, select_trigger_with_focus_visible,
    },
    separator::{SeparatorOrientation, separator},
    slider::{SliderView, slider, slider_pointer_percent},
    text_input::{
        TextInputContentAlign, TextInputView, text_input, text_input_anchor_probe,
        text_input_value_segments, text_input_with_content_align,
    },
};
use oxideterm_i18n::I18n;
use oxideterm_network_proxy::install_application_proxy_policy_from_settings;
use oxideterm_session_adapter::upstream_proxy_config_from_global_settings;

#[derive(Clone, Debug)]
pub(in crate::workspace) enum SettingsManagedKeyDialog {
    ImportFile,
    Paste,
    Rename {
        key_id: String,
    },
    Delete {
        key: ManagedSshKeyInfo,
        usage: ManagedSshKeyUsage,
    },
}

/// Keeps a settings dialog mounted during its exit animation while an inert
/// overlay prevents the retained form payload from receiving more input.
pub(in crate::workspace) fn settings_dialog_transition(
    tokens: &ThemeTokens,
    animation_id: &'static str,
    backdrop: Div,
    form: Div,
    phase: oxideterm_gpui_ui::motion::ExitPhase,
) -> AnyElement {
    let is_visible = phase == oxideterm_gpui_ui::motion::ExitPhase::Visible;
    backdrop
        .child(oxideterm_gpui_ui::motion::form_transition(
            tokens,
            animation_id,
            form,
            is_visible,
        ))
        .when(!is_visible, settings_dialog_inert_overlay)
        .into_any_element()
}

pub(in crate::workspace) fn settings_dialog_inert_overlay(backdrop: Div) -> Div {
    backdrop.child(
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .occlude()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .on_scroll_wheel(|_event, _window, cx| cx.stop_propagation()),
    )
}

pub(in crate::workspace) fn settings_store_modified_time(
    path: &std::path::Path,
) -> Option<std::time::SystemTime> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
}

pub(in crate::workspace) const APPEARANCE_BORDER_RADIUS_MIN: f32 = 0.0; // Tauri AppearanceTab Slider min={0}.
pub(in crate::workspace) const APPEARANCE_BORDER_RADIUS_MAX: f32 = 16.0; // Tauri AppearanceTab Slider max={16} and settings normalization.
pub(in crate::workspace) const APPEARANCE_UI_FONT_SIZE_MIN: f32 = 11.0;
pub(in crate::workspace) const APPEARANCE_UI_FONT_SIZE_MAX: f32 = 20.0;

mod appearance;
mod cards;
mod connections_page;
mod controls;
mod entity;
pub(in crate::workspace) use entity::{
    BackgroundGalleryOperationResult, ConnectionImportSnapshot, DataDirectoryConfirm,
    DataDirectoryOperationResult, LaunchAtLoginError, ManagedKeyDialogSnapshot,
    NetworkProxyPasswordSnapshot, NetworkProxyTestSnapshot, SettingsNavigationDraftAction,
    SettingsWorkspaceEntity,
    SettingsWorkspaceEvent, SettingsWorkspaceToast, SshConfigImportSnapshot,
};
mod general_terminal_pages;
mod highlight;
mod local_terminal;
mod navigation_editor;
mod network_page;
mod pages;
mod search;
mod sftp_page;
mod surface;
mod terminal_controls;
mod terminal_display;
mod terminal_triggers;
pub(in crate::workspace) use terminal_triggers::TerminalTriggersSettingsState;
mod update;
mod update_ui;

use connections_page::{
    connection_idle_timeout_options, connection_import_duplicate_strategy_label,
    connection_import_source_label, connection_import_source_options, session_export_format_label,
    session_export_format_options,
};
use network_page::{
    NetworkProxyAuthMode, network_application_proxy_mode_label, network_proxy_auth_label,
    network_proxy_protocol_label,
};
pub(in crate::workspace) use update::{
    NativeUpdateRenderState, native_update_progress_hint, native_update_progress_ratio,
};

fn settings_tab_lucide(icon: SettingsTabIcon) -> LucideIcon {
    match icon {
        SettingsTabIcon::BookOpen => LucideIcon::BookOpen,
        SettingsTabIcon::HardDrive => LucideIcon::HardDrive,
        SettingsTabIcon::HelpCircle => LucideIcon::HelpCircle,
        SettingsTabIcon::Monitor => LucideIcon::Monitor,
        SettingsTabIcon::Network => LucideIcon::Network,
        SettingsTabIcon::Shield => LucideIcon::Shield,
        SettingsTabIcon::Sparkles => LucideIcon::Sparkles,
        SettingsTabIcon::Square => LucideIcon::Square,
        SettingsTabIcon::Terminal => LucideIcon::Terminal,
        SettingsTabIcon::WifiOff => LucideIcon::WifiOff,
    }
}

fn settings_background_tab_lucide(icon: SettingsBackgroundTabIcon) -> LucideIcon {
    match icon {
        SettingsBackgroundTabIcon::Activity => LucideIcon::Activity,
        SettingsBackgroundTabIcon::ArrowLeftRight => LucideIcon::ArrowLeftRight,
        SettingsBackgroundTabIcon::Bell => LucideIcon::Bell,
        SettingsBackgroundTabIcon::Cloud => LucideIcon::Cloud,
        SettingsBackgroundTabIcon::FolderInput => LucideIcon::FolderInput,
        SettingsBackgroundTabIcon::Gauge => LucideIcon::Gauge,
        SettingsBackgroundTabIcon::ListTree => LucideIcon::ListTree,
        SettingsBackgroundTabIcon::Monitor => LucideIcon::Monitor,
        SettingsBackgroundTabIcon::Network => LucideIcon::Network,
        SettingsBackgroundTabIcon::Puzzle => LucideIcon::Puzzle,
        SettingsBackgroundTabIcon::Rocket => LucideIcon::Rocket,
        SettingsBackgroundTabIcon::Settings => LucideIcon::Settings,
        SettingsBackgroundTabIcon::Terminal => LucideIcon::Terminal,
    }
}
