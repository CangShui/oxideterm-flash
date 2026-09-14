mod actions;
mod breadcrumb_scroll;
mod browser_behavior;
mod command_palette;
mod connection_monitor;
mod cloud_sync;
mod delivery;
mod detached_tab_window;
mod forwards;
mod ime;
mod launcher;
mod new_connection;
mod onboarding;
mod overlay;
mod pane_tree;
mod path_completion;
mod quick_commands;
mod remote_desktop;
mod runtime_entity;
mod root {
    pub(super) mod background;
    pub(super) mod helpers;
    pub(super) mod host_tools;
    pub(super) mod init;
    pub(super) mod modal_owner;
    pub(super) mod render;
    pub(super) mod state;
    #[cfg(test)]
    pub(super) mod tests;
    pub(super) mod window_state;
}
mod selectable_text;
mod selection_motion;
mod session_icons;
mod connection_workspace;
mod settings;
mod sftp;
mod sidebar;
mod tabs;
mod terminal_cast;
mod terminal_command_bar;
mod terminal_command_sender;
mod terminal_context_actions;
mod terminal_cwd;
mod terminal_entity;
mod terminal_git;
mod terminal_project;
mod terminal_triggers_runtime;
mod ui_palette;
mod version_migration;
mod virtual_list;
mod window_intent;
mod window_registry;
mod window_shell;

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet, VecDeque, hash_map::DefaultHasher},
    fs,
    hash::{Hash, Hasher},
    io,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};

use self::{
    breadcrumb_scroll::scroll_breadcrumb_by_wheel,
    path_completion::{
        PathCompletionCandidate, PathCompletionOwner, PathCompletionState,
        local_path_completion_request, remote_path_completion_request,
    },
    sidebar::{ContextSidebarPanel, ContextSidebarTool},
    version_migration::VersionMigrationState,
};
use anyhow::Result;
use gpui::{
    AnchoredPositionMode, Animation, AnimationExt, AnyElement, AnyWindowHandle, App, Bounds,
    ClipboardEntry, ClipboardItem, Context, Corner, CursorStyle, Entity, FocusHandle, Focusable,
    Image, ImageFormat, IntoElement, KeyDownEvent, KeyUpEvent, ListAlignment,
    ListState, ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ObjectFit, ParentElement, PathPromptOptions, Pixels, Point, Render, RenderImage, Rgba,
    ScrollHandle, ScrollWheelEvent, SharedString, Styled, StyledImage, Subscription, Task,
    TextLayout, Timer, Window, anchored, canvas, deferred, div,
    prelude::*, px, relative, rgb, rgba, svg,
};
use oxideterm_connection_monitor::{
    CompactMonitorRow, ConnectionPoolEntryState,
    DockerActionKind, FilesystemCommandCapability,
    FilesystemEntrySeverity, FilesystemFilter, GpuDevice, GpuProvider, GpuSamplingTask,
    GpuSnapshot, GpuSnapshotStatus, GpuUpdate, LogCommandCapability, LogPreset, MetricsSource,
    MonitorMetricKind, MonitorSectionKind, MonitorValueLevel, PackageCommandCapability,
    PackageFilter, PortCommandCapability, PortFilter, ProcessActionKind, ProcessCommandCapability,
    ProcessFilter, ProcessSort, ProfilerRegistry, ProfilerUpdate, ResourceDockerContainer,
    ResourceDockerStatus, ResourceFilesystemEntry, ResourceFilesystemSnapshot,
    ResourceFilesystemStatus, ResourceLogEntry, ResourceLogSnapshot, ResourceLogStatus,
    ResourceMetrics, ResourcePackageEntry, ResourcePackageSnapshot, ResourcePackageStatus,
    ResourcePortEntry, ResourcePortSnapshot, ResourcePortStatus, ResourceScheduledTask,
    ResourceScheduledTaskSnapshot, ResourceScheduledTaskStatus, ResourceScreenSnapshot,
    ResourceService, ResourceServiceStatus, ResourceTmuxSnapshot, ResourceTopProcess,
    ScheduledTaskActionKind, ScheduledTaskCapability, ScheduledTaskFilter, ServiceActionKind,
    ServiceCommandCapability, VirtualTerminalEngine, build_docker_action_command,
    build_docker_exec_shell_command, build_docker_follow_logs_command, build_docker_logs_command,
    build_filesystem_diagnostic_command, build_filesystem_snapshot_command,
    build_log_follow_command, build_log_snapshot_command, build_package_inspect_command,
    build_package_snapshot_command, build_port_diagnostic_command, build_port_snapshot_command,
    build_process_action_command, build_scheduled_task_action_command,
    build_scheduled_task_diagnostic_command, build_scheduled_task_logs_command,
    build_scheduled_task_snapshot_command, build_service_action_command,
    build_service_follow_logs_command, build_service_logs_command,
    build_tmux_rename_session_command, build_tmux_rename_window_command,
    build_tmux_send_pane_command, compact_monitor_row_signature, compact_monitor_rows,
    docker_action_succeeded, docker_row_signature, docker_state_label_key,
    filesystem_attention_label_keys, filesystem_entry_severity, filesystem_filter_label_key,
    filesystem_kind_label_key, filesystem_read_only_label_key, filesystem_row_signature,
    format_bytes, gpu_device_row_signature, log_level_label_key, log_preset_label_key,
    log_row_signature, package_filter_label_key, package_row_signature,
    package_status_label_key, parse_log_snapshot, parse_package_snapshot, parse_port_snapshot,
    percent_level, port_endpoint, port_filter_label_key, port_is_risky_exposure,
    port_row_signature, port_state_label_key, process_display_command, process_display_name,
    process_row_signature, process_state_label_key, scheduled_task_active_label_key,
    scheduled_task_enabled_label_key, scheduled_task_filter_label_key,
    scheduled_task_row_signature, scheduled_task_source_label_key, service_action_succeeded,
    service_enabled_label_key, service_row_signature, service_state_label_key,
    start_gpu_sampling_on, visible_docker_rows, visible_filesystem_rows, visible_log_rows,
    visible_package_rows, visible_port_rows, visible_process_rows, visible_scheduled_task_rows,
    visible_service_rows,
};
use oxideterm_connections::{
    ConnectionStore, ConnectionTerminalOptions, SaveConnectionRequest,
    SessionExportFormat, SshConfigSyncService,
};
use oxideterm_forwarding::{
    ForwardEventDeliverySender, ForwardStatus, ForwardingRegistry, SavedForwardStore,
};
use oxideterm_gpui_platform::{
    rendering::detect_graphics,
    vibrancy::{NativeVibrancyMode, VibrancySupport, apply_window_vibrancy},
    window_opacity::{apply_window_opacity, normalized_window_opacity},
};
use oxideterm_gpui_terminal::{
    BackgroundImageRenderCache, SemanticShellDialect, SharedTerminalSession,
    TerminalBackgroundFit, TerminalBackgroundPreferences, TerminalBroadcastInputKind,
    TerminalCommandSelectionLabels, TerminalContextAction, TerminalHighlightMatchScope,
    TerminalHighlightRenderMode, TerminalHighlightRule as UiHighlightRule,
    TerminalHighlightRuleSetOverride, TerminalInputBroadcaster,
    TerminalModemLabels, TerminalNotice, TerminalNoticeVariant,
    TerminalPane, TerminalPaneEvent, TerminalPasteLabels,
    TerminalRecordingState, TerminalRecordingStatus, TerminalSearchStatus,
    TerminalSerialControlLabels, TerminalTrzszLabels, TerminalUiPreferenceOverrides,
    TerminalUiPreferences, TerminalUiTheme, TerminalWorkingDirectorySource,
    resolved_terminal_semantic_scheme,
};
use oxideterm_gpui_ui::scroll::ScrollableElement;
use oxideterm_gpui_ui::{
    ConfirmDialogAction, ConfirmDialogVariant, ConfirmDialogView, checkbox,
    modal::set_tauri_backdrop_blur_allowed,
    text_input::{TextInputAnchorId, TextInputView, text_input, text_input_anchor_probe},
    toast::{ToastVariant, ToastView, toast_action, toast_close},
    toaster::toaster,
    tooltip::tooltip_content,
};
use oxideterm_i18n::{I18n, Locale};
use oxideterm_render_policy::{
    DetectedGraphics, EffectiveRenderPolicy, RenderProfile, compute_render_policy,
};
use oxideterm_session_adapter::{
    sftp_runtime_settings_from_settings, terminal_backspace_sequence_from_connection,
    terminal_delete_sequence_from_connection, terminal_encoding_from_connection,
    terminal_encoding_from_settings as session_terminal_encoding,
};
use oxideterm_settings::{
    AI_SIDEBAR_ABSOLUTE_MAX_WIDTH, AI_SIDEBAR_ABSOLUTE_MIN_WIDTH, BackgroundFit, BackgroundScope,
    CursorStyle as SettingsCursorStyle, FrostedGlassMode, GLOBAL_HIGHLIGHT_RULE_SET_ID,
    HighlightRule, HighlightRuleMatchScope, HighlightRuleRenderMode, Language,
    MAX_TERMINAL_BACKGROUND_OPACITY, MAX_WINDOW_OPACITY, MIN_TERMINAL_BACKGROUND_OPACITY,
    MIN_WINDOW_OPACITY, PersistedSettings, SettingsStore,
    default_settings_path, list_background_images,
};
use oxideterm_settings_model::SettingsNavigationLayout;
use oxideterm_sftp::{LazyProgressStore, ProgressStore, RemoteRelayDisposition, SftpTransferManager, StoredTransferProgress};
use oxideterm_ssh::{
    AuthMethod, ConnectionConsumer, ConnectionPoolConfig, ConnectionProgressReporter,
    ConnectionState, ConnectionTraceEvent, ConnectionTraceMode, ConnectionTracePlan,
    ConnectionTraceStage, ConnectionTraceState, ConnectionTraceStatus, MAX_RETAINED_RECONNECT_JOBS,
    NodeEventReceiver, NodeEventSubscription, NodeId, NodeOrigin, NodeReadiness, NodeRouter,
    NodeRuntimeStore, NodeState, NodeStateEvent, NodeTreeExpansion, NodeTreePersistenceSnapshot,
    NodeTreeSnapshot, NodeTreeSnapshotNode, PhaseResult, ProbeConnectionStatus, ProxyHopConfig,
    ReconnectForwardRuleSnapshot, ReconnectNodeConnectionSnapshot, ReconnectNodeTerminalSnapshot,
    ReconnectNodeTransferSnapshot, ReconnectOrchestratorStore, ReconnectPhase, ReconnectProgress,
    ReconnectSnapshot, SshAlgorithmDiagnosticKind, SshConfig, SshConnectionHandle,
    SshConnectionRegistry, SshTransportClient, SshTransportError, TerminalEndpoint,
};
use oxideterm_ssh_launch::{NativeConnectionLaunch, TemporarySshLaunch, TemporaryTelnetLaunch};
use oxideterm_terminal::{
    SerialSessionConfig, ShellInfo,
    SshSessionConfig, TelnetSessionConfig, TerminalCommandMarkDetectionSource, TerminalCursorShape,
    TerminalLifecycle,
};
use oxideterm_theme::{
    AppUiColors, ThemeTokens, UiDensityProfile, UiMotionProfile, UiRadii,
    theme_by_id,
};
use oxideterm_workspace::{
    ActiveSessionNode, ActiveSessionReadiness, ActiveSessionStatus, MAX_PANES_PER_TAB, PaneId,
    PaneNode, SplitDirection, Tab, TabId, TabKind, TabTitleSource, TerminalSessionId,
    adjusted_split_sizes,
};

use self::actions::SearchBarState;
use self::connection_monitor::{
    HostToolsEntity, HostToolsEvent, HostToolsMessages, HostToolsWindowIntent,
    HostToolsWindowRequest,
};
use self::ime::{
    HostToolsPlainTextImeFrame, TextInputAnchorStore, WorkspaceImeDragSelection,
    WorkspaceImeElement, WorkspaceImeSelection, WorkspaceImeTarget,
    active_ime_should_defer_input_key, workspace_ime_target_for_plain_host_tools_input,
};
use self::launcher::{LauncherWorkspaceEntity, LauncherWorkspaceEvent};
use self::new_connection::{
    ConnectionFlowEntity, ConnectionFlowEvent, NativeSshPromptHandler, NewConnectionField,
    NewConnectionForm, SshAuthTab, SshConnectionIntent,
};
use self::onboarding::OnboardingState;
use self::overlay::{
    WorkspaceOverlayConfirmEffect, WorkspaceOverlayConfirmKeyAction, WorkspaceOverlayConfirmKind,
    WorkspaceOverlayEntity, WorkspaceOverlayIntent,
};
use self::pane_tree::SplitDrag;
use self::root::state::{ReconnectWorkerResult, WorkspaceSshNode, WorkspaceSshNodeEndpoint};
use self::root::{background::*, helpers::*};
use self::connection_workspace::{ConnectionWorkspaceState, ConnectionWorkspaceEvent};
use self::sidebar::{
    ActiveSessionContextMenu, ActiveSessionFolderContextMenu, ActiveSessionSidebarViewMode,
    MoveSessionFolderDialogState, NewSessionFolderDialogState, SidebarSection,
};
use self::tabs::TerminalLocation;
use self::terminal_entity::{WorkspaceTerminalEntity, WorkspaceTerminalEvent};
use self::window_intent::WorkspaceWindowIntentEntity;
use crate::{
    CloseOtherTabs, ClosePane, CloseSearch, CloseTab, CommandPalette, Copy, Cut, Find, FindNext,
    FindPrev, FontDecrease, FontIncrease, FontReset, GoToTab1, GoToTab2, GoToTab3, GoToTab4,
    GoToTab5, GoToTab6, GoToTab7, GoToTab8, GoToTab9, NewConnection, NewTerminal, NextTab,
    OpenSettings, PaletteBroadcast, PaletteCancelReconnect,
    PaletteDisconnectAll, PaletteHealthCheck,
    PaletteReconnectAll, PaletteResetPanes, Paste, PrevTab,
    SplitHorizontal, SplitNavLeft, SplitNavRight, SplitVertical, SwitchLocaleChinese,
    SwitchLocaleEnglish, TerminalClearScreen,
    TerminalFreeTypeMode, TerminalRecording, ToggleFullscreen, ToggleSidebar, ZenMode,
};
use crate::assets::LucideIcon;
use oxideterm_gpui_markdown::{
    MarkdownCodeBlockActions, MarkdownMermaidZoomHandler,
    MarkdownOptions, MarkdownVirtualListScrollHandle, markdown_virtual_with_code_actions,
};

const MERMAID_MODAL_RASTER_SCALE: f32 = 3.0;

pub(crate) fn locale_from_settings(language: Language) -> Locale {
    root_locale_from_settings(language)
}

use oxideterm_gpui_settings_view::{
    ActiveSurface, SettingsInput, SettingsSelect, SettingsSlider, SettingsTab,
};
use oxideterm_gpui_ui::select::{OverlayAnchor, SelectAnchorId, select_anchor_probe};
use oxideterm_gpui_ui::text_input::TextInputAnchor;
use oxideterm_gpui_ui::typography::{
    css_font_family_head as settings_css_font_family_head, gpui_font_family_name,
    tauri_ui_font_family as settings_ui_font_family,
};
pub(super) use selectable_text::{
    SelectableTextRole, SelectableTextScrollExt, selectable_vertical_scrollbar_layer,
};
pub(super) use virtual_list::{
    TauriVirtualListSpec, TauriVirtualScrollAlign, scroll_tauri_virtual_list_to_index,
    tauri_virtual_list, tauri_virtual_list_state,
    tauri_virtual_uniform_list, uniform_list_edge_autoscroll,
};
use virtual_list::{
    VirtualListSignatureCache, sync_tauri_variable_list_state_by_signatures,
};

const SETTINGS_SECTION_LIST_INITIAL_ITEM_COUNT: usize = 4;
const SETTINGS_PERCENT_SCALE: f64 = 100.0;
const SETTINGS_SECTION_LIST_ESTIMATED_HEIGHT: f32 = 260.0;
const SETTINGS_SECTION_LIST_OVERSCAN: usize = 2;
const SETTINGS_SCROLL_CARET_PAUSE_MS: u64 = 700;
const FORWARDS_SECTION_LIST_INITIAL_ITEM_COUNT: usize = 5;
const FORWARDS_SECTION_LIST_ESTIMATED_HEIGHT: f32 = 180.0;
const FORWARDS_SECTION_LIST_OVERSCAN: usize = 2;
const FORWARDS_TABLE_ROW_LIST_INITIAL_ITEM_COUNT: usize = 0;
const FORWARDS_TABLE_ROW_LIST_ESTIMATED_HEIGHT: f32 = 42.0;
const FORWARDS_TABLE_ROW_LIST_OVERSCAN: usize = 8;
const QUICK_COMMAND_LIST_INITIAL_ITEM_COUNT: usize = 0;
const QUICK_COMMAND_LIST_ESTIMATED_HEIGHT: f32 = 56.0;
const QUICK_COMMAND_LIST_OVERSCAN: usize = 6;
const ACTIVE_SESSION_SIDEBAR_LIST_INITIAL_ITEM_COUNT: usize = 0;
const ACTIVE_SESSION_SIDEBAR_LIST_ESTIMATED_HEIGHT: f32 = 40.0;
const ACTIVE_SESSION_SIDEBAR_LIST_OVERSCAN: usize = 8;
#[allow(dead_code)]
const ACTIVE_SESSION_FOCUS_LIST_ESTIMATED_HEIGHT: f32 = 76.0;
const OXIDE_EXPORT_CONNECTION_LIST_INITIAL_ITEM_COUNT: usize = 0;
const OXIDE_EXPORT_CONNECTION_LIST_ESTIMATED_HEIGHT: f32 = 58.0;
const OXIDE_EXPORT_CONNECTION_LIST_OVERSCAN: usize = 8;
const OXIDE_IMPORT_CONNECTION_PREVIEW_LIST_INITIAL_ITEM_COUNT: usize = 0;
const OXIDE_IMPORT_CONNECTION_PREVIEW_LIST_ESTIMATED_HEIGHT: f32 = 22.0;
const OXIDE_IMPORT_CONNECTION_PREVIEW_LIST_OVERSCAN: usize = 8;
const OXIDE_EXPORT_FORWARD_GROUP_LIST_INITIAL_ITEM_COUNT: usize = 0;
const OXIDE_EXPORT_FORWARD_GROUP_LIST_ESTIMATED_HEIGHT: f32 = 84.0;
const OXIDE_EXPORT_FORWARD_GROUP_LIST_OVERSCAN: usize = 4;
const OXIDE_EXPORT_SUMMARY_LINE_LIST_INITIAL_ITEM_COUNT: usize = 0;
const OXIDE_EXPORT_SUMMARY_LINE_LIST_ESTIMATED_HEIGHT: f32 = 18.0;
const OXIDE_EXPORT_SUMMARY_LINE_LIST_OVERSCAN: usize = 6;
const OXIDE_IMPORT_FORWARD_DETAIL_LIST_INITIAL_ITEM_COUNT: usize = 0;
const OXIDE_IMPORT_FORWARD_DETAIL_LIST_ESTIMATED_HEIGHT: f32 = 36.0;
const OXIDE_IMPORT_FORWARD_DETAIL_LIST_OVERSCAN: usize = 6;
const OXIDE_IMPORT_NAME_GROUP_LIST_INITIAL_ITEM_COUNT: usize = 0;
const OXIDE_IMPORT_NAME_GROUP_LIST_ESTIMATED_HEIGHT: f32 = 28.0;
const OXIDE_IMPORT_NAME_GROUP_LIST_OVERSCAN: usize = 6;

const CONFIRM_DIALOG_FOOTER_ACTIONS: [ConfirmDialogAction; 2] =
    [ConfirmDialogAction::Cancel, ConfirmDialogAction::Confirm];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConfirmKeyboardAction {
    Cancel,
    Confirm,
    Handled,
}


#[derive(Clone, Debug, Eq, PartialEq)]
enum TabDragMode {
    Pending,
    Reorder,
    Detach,
}

#[derive(Clone, Debug)]
struct TabDragState {
    tab_id: TabId,
    from_index: usize,
    start_x: f32,
    start_y: f32,
    current_x: f32,
    current_y: f32,
    tab_widths: Vec<f32>,
    active: bool,
    mode: TabDragMode,
    drop_target_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TabContextMenu {
    tab_id: TabId,
    x: f32,
    y: f32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TabRenameDialog {
    // The dialog edits display metadata for one canonical terminal tab only.
    tab_id: TabId,
    draft: String,
}

#[derive(Clone, Debug)]
struct ExitingTabVisual {
    tab_id: TabId,
    kind: TabKind,
    title: String,
    width: f32,
    visual_index: usize,
    was_active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum TabCloseConfirm {
    Single { tab_id: TabId },
    LocalChildProcess { tab_id: TabId },
    LocalChildProcessBatch { tab_ids: Vec<TabId> },
    Other { tab_ids: Vec<TabId> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum LocalTerminalCloseCheck {
    Single { tab_id: TabId },
    Batch { tab_ids: Vec<TabId> },
}

impl LocalTerminalCloseCheck {
    fn tab_ids(&self) -> Vec<TabId> {
        match self {
            Self::Single { tab_id } => vec![*tab_id],
            Self::Batch { tab_ids } => tab_ids.clone(),
        }
    }
}

struct WorkspaceWindowTabState {
    drag: Option<TabDragState>,
    context_menu: Option<TabContextMenu>,
    exiting_tabs: Vec<ExitingTabVisual>,
    scroll_handle: ScrollHandle,
    scrollbar_drag: Option<TabbarScrollbarDragState>,
    scrollbar_hovered: bool,
}

#[derive(Clone, Copy, Debug)]
struct TabbarScrollbarDragState {
    // Preserve the pointer's position inside the thumb to prevent a jump on drag start.
    grab_offset_x: f32,
}

#[derive(Clone, Copy, Debug)]
struct DetachedTabReturnDrag {
    tab_id: TabId,
    start_screen_x: f32,
    start_screen_y: f32,
    current_screen_x: f32,
    current_screen_y: f32,
    active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TabWindowHandoffOrigin {
    screen_left: f32,
    screen_top: f32,
    width: f32,
    height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DetachedTabReturnHandoff {
    tab_id: TabId,
    origin: TabWindowHandoffOrigin,
    generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DetachedTabReturnPlaceholder {
    tab_id: TabId,
    visible_index: usize,
}

impl WorkspaceWindowTabState {
    fn new() -> Self {
        Self {
            drag: None,
            context_menu: None,
            exiting_tabs: Vec::new(),
            scroll_handle: ScrollHandle::new(),
            scrollbar_drag: None,
            scrollbar_hovered: false,
        }
    }
}

#[derive(Clone)]
pub(super) struct SelectableTextFragmentState {
    pub group_id: u64,
    pub order: usize,
    pub generation: u64,
    pub text: String,
    pub layout: TextLayout,
    pub anchor: TextInputAnchor,
}

pub(crate) struct WorkspaceApp {
    focus_handle: FocusHandle,
    main_window_tabs: WorkspaceWindowTabState,
    tab_rename_dialog: Option<TabRenameDialog>,
    detached_tab_return_drag: Option<DetachedTabReturnDrag>,
    detached_tab_return_handoff: Option<DetachedTabReturnHandoff>,
    next_tab_window_handoff_generation: u64,
    main_window_tabbar_drop_bounds: Option<Bounds<Pixels>>,
    pending_auto_close_terminal_sessions: HashSet<TerminalSessionId>,
    auto_close_terminal_sessions_scheduled: bool,
    tab_host: Entity<tabs::WorkspaceTabHostEntity>,
    _tab_host_subscription: Subscription,
    search: SearchBarState,
    terminal_recording_menu_open: bool,
    terminal_highlight_popover_open: bool,
    // Settings keep the source pane stable while editing session-only trigger overrides.
    terminal_trigger_settings_pane: Option<PaneId>,
    terminal_trigger_shell_confirmation_pending: bool,
    terminal_triggers: settings::TerminalTriggersSettingsState,
    terminal_trigger_saved_connections:
        HashMap<TerminalSessionId, oxideterm_terminal_triggers::SavedConnectionRef>,
    terminal_semantic_highlight_section_expanded: bool,
    terminal_rule_highlight_section_expanded: bool,
    terminal_command_context_highlight_section_expanded: bool,
    terminal_command_sender: Entity<terminal_command_sender::TerminalCommandSenderEntity>,
    _terminal_command_sender_observation: Subscription,
    serial_terminal_configs: HashMap<TerminalSessionId, SerialSessionConfig>,
    // A Telnet pane keeps only the stable profile owner needed for toolbar persistence.
    telnet_terminal_profile_ids: HashMap<TerminalSessionId, String>,
    /// Maps a running serial terminal session back to its saved profile so the
    /// sidebar can claim the profile's row instead of rendering a duplicate.
    serial_terminal_profile_ids: HashMap<TerminalSessionId, String>,
    command_palette: Entity<command_palette::CommandPaletteEntity>,
    _command_palette_observation: Subscription,
    version_migration: VersionMigrationState,
    onboarding: OnboardingState,
    settings_workspace: Entity<settings::SettingsWorkspaceEntity>,
    _settings_workspace_observation: Subscription,
    _settings_workspace_subscription: Subscription,
    segmented_control_user_motion: selection_motion::UserSegmentedControlMotionState,
    split_drag: Option<SplitDrag>,
    // Last pointer position inside the command palette list; filters out
    // hover refires caused by keyboard scrolling rows under a still pointer.
    command_palette_hover_position: Option<gpui::Point<Pixels>>,
    // Measured cross-axis extents of split group containers, probed each
    // paint so divider drags divide by the container, not the window.
    split_group_extents: HashMap<PaneId, f32>,
    sidebar_resizing: bool,
    sidebar_resize_trace_id: Option<u64>,
    // Tracks the Host Tools (context) sidebar drag separately so the shared
    // cursor-override logic never drives the left sidebar width from it.
    context_sidebar_resizing: bool,
    context_sidebar_resize_trace_id: Option<u64>,
    embedded_sftp_sidebar_resizing: bool,
    sidebar_resize_hotzone_hovered: bool,
    sidebar_collapsed: bool,
    sidebar_rendered: bool,
    sidebar_motion_generation: u64,
    sidebar_width: f32,
    context_sidebar_rendered: bool,
    context_sidebar_motion_generation: u64,
    active_context_sidebar_panel: ContextSidebarPanel,
    needs_active_pane_focus: bool,
    active_sidebar_section: SidebarSection,
    active_surface: ActiveSurface,
    #[allow(dead_code)]
    active_session_sidebar_view_mode: ActiveSessionSidebarViewMode,
    #[allow(dead_code)]
    active_session_sidebar_focused_node_id: Option<NodeId>,
    active_session_context_menu: Option<ActiveSessionContextMenu>,
    active_session_folder_context_menu: Option<ActiveSessionFolderContextMenu>,
    session_folder_delete_pending: Option<String>,
    move_session_folder_dialog: Option<MoveSessionFolderDialogState>,
    new_session_folder_dialog: Option<NewSessionFolderDialogState>,
    active_session_sidebar_list_state: ListState,
    active_session_sidebar_list_cache: RefCell<VirtualListSignatureCache>,
    // The virtual list item callbacks rebuild rows per visible item unless the
    // container keeps one materialized snapshot per view mode. Rebuilding the
    // node tree and connection catalog once per frame, not once per row, is the
    // difference between a smooth scroll and a stalled sidebar.
    active_session_sidebar_rows_cache:
        RefCell<Option<(ActiveSessionSidebarViewMode, Vec<crate::workspace::sidebar::ActiveSessionSidebarRow>)>>,
    open_settings_select: Option<SettingsSelect>,
    settings_select_focus_origin: Option<browser_behavior::BrowserFocusOrigin>,
    settings_section_list_state: ListState,
    settings_section_list_cache: RefCell<VirtualListSignatureCache>,
    standard_confirm_focused_action: Option<ConfirmDialogAction>,
    skip_future_ssh_close_confirmations: bool,
    select_anchors: HashMap<SelectAnchorId, OverlayAnchor>,
    text_input_anchors: TextInputAnchorStore,
    selectable_text_values: HashMap<u64, String>,
    selectable_text_layouts: HashMap<u64, TextLayout>,
    selectable_text_fragments: HashMap<u64, SelectableTextFragmentState>,
    selectable_text_generation: u64,
    selectable_text_pending_updates: Rc<RefCell<selectable_text::SelectableTextFrameUpdates>>,
    selectable_text_flush_scheduled: Rc<Cell<bool>>,
    selectable_text_autoscroll_position: Option<Point<Pixels>>,
    selectable_text_autoscroll_scheduled: bool,
    selectable_text_scroll_handles: RefCell<HashMap<String, ScrollHandle>>,
    mermaid_zoom: Option<MermaidZoomState>,
    ime_marked_text: Option<ime::WorkspaceImeMarkedText>,
    pending_platform_text_commit: Option<ime::PendingPlatformTextCommit>,
    next_platform_text_commit_generation: u64,
    selected_ime_target: Option<WorkspaceImeTarget>,
    selected_ime_range: Option<WorkspaceImeSelection>,
    ime_drag_selection: Option<WorkspaceImeDragSelection>,
    focused_settings_input: Option<SettingsInput>,
    settings_input_draft: String,
    // The large command-spec document is edited in a workspace modal so the
    // settings virtual list remains the only scroll owner behind it.
    terminal_command_specs_editor_open: bool,
    settings_slider_drag: Option<SettingsSlider>,
    // Slider drags update live state on every pointer move; the disk write is
    // coalesced into one save when the drag finishes.
    settings_save_pending: bool,
    // Inputs whose current draft failed validation, so the rejection toast
    // fires once per transition instead of on every keystroke.
    invalid_settings_input: HashSet<SettingsInput>,
    workspace_input: Entity<ime::WorkspaceInputEntity>,
    _workspace_input_observation: Subscription,
    input_caret: ime::WorkspaceCaretVisibility,
    native_update_notification_open: bool,
    native_update_notification_presence: oxideterm_gpui_ui::motion::ExitPresence,
    native_update_release_notes_scroll: MarkdownVirtualListScrollHandle,
    settings_legal_notice_scroll: MarkdownVirtualListScrollHandle,
    _window_intents: Entity<WorkspaceWindowIntentEntity>,
    _window_intent_subscription: Subscription,
    window_registry: window_registry::WorkspaceWindowRegistry,
    window_effect_delivery_scheduled: bool,
    connection_flow: Entity<ConnectionFlowEntity>,
    _connection_flow_observation: Subscription,
    _connection_flow_subscription: Subscription,
    workspace_runtime: Entity<runtime_entity::WorkspaceRuntimeEntity>,
    _workspace_runtime_subscription: Subscription,
    ssh_registry: SshConnectionRegistry,
    forwarding_service: forwards::ForwardingRuntimeService,
    forwarding_runtime: Arc<tokio::runtime::Runtime>,
    sftp_transfer_manager: Arc<SftpTransferManager>,
    cloud_sync: Option<cloud_sync::CloudSyncRuntime>,
    cloud_sync_config: Option<cloud_sync::ResolvedCloudSyncConfig>,
    cloud_sync_status: Option<String>,
    cloud_sync_generation: u64,
    cloud_sync_logs: VecDeque<String>,
    cloud_sync_progress: Option<f32>,
    cloud_sync_delete_prompt: Option<cloud_sync::CloudSyncDeletePrompt>,
    cloud_sync_auto_push_task: Option<Task<()>>,
    cloud_sync_observed_store_state: Option<cloud_sync::CloudSyncStoreState>,
    sftp_progress_store: Arc<dyn ProgressStore>,
    node_router: NodeRouter,
    ssh_nodes: HashMap<NodeId, WorkspaceSshNode>,
    saved_ssh_nodes: HashMap<String, NodeId>,
    expanded_ssh_nodes: HashSet<NodeId>,
    active_ssh_node_id: Option<NodeId>,
    next_ssh_node_id: u64,
    forwarding: Entity<forwards::ForwardingWorkspaceEntity>,
    _forwarding_subscriptions: Vec<Subscription>,
    sftp_tab_nodes: HashMap<TabId, NodeId>,
    standalone_sftp_tabs: HashMap<TabId, sftp::StandaloneSftpTabBinding>,
    standalone_sftp_sessions: HashMap<String, sftp::StandaloneSftpRuntime>,
    pending_standalone_sftp_pair_launches:
        HashMap<String, new_connection::PendingStandaloneSftpPairLaunch>,
    embedded_sftp_node_id: Option<NodeId>,
    // A manual SFTP close suppresses automatic rebinding until the user
    // presses reconnect or explicitly starts the SSH connection again.
    sftp_manually_closed_node_id: Option<NodeId>,
    sftp_presentation_request: Option<sftp::SftpPresentationRequest>,
    sftp_view: Entity<sftp::SftpWorkspaceEntity>,
    _sftp_observation: Subscription,
    _sftp_subscription: Subscription,
    launcher: Entity<LauncherWorkspaceEntity>,
    _launcher_observation: Subscription,
    _launcher_subscription: Subscription,
    host_tools: Entity<HostToolsEntity>,
    _host_tools_subscription: Subscription,
    i18n: I18n,
    tokens: ThemeTokens,
    detected_graphics: DetectedGraphics,
    render_profile_override: Option<RenderProfile>,
    render_policy: EffectiveRenderPolicy,
    vibrancy_support: VibrancySupport,
    settings_store: SettingsStore,
    pending_window_ui_state: Option<oxideterm_settings::WindowUiState>,
    window_state_save_task: Option<Task<()>>,
    // Font-size shortcuts fire in bursts; each step applies live while the
    // disk write coalesces behind this debounce, mirroring window-state saves.
    pending_terminal_font_size: Option<i64>,
    terminal_font_size_save_task: Option<Task<()>>,
    connection_store: ConnectionStore,
    // The connection-layer worker owns SSH config parsing and persistence.
    ssh_config_sync_service: Option<SshConfigSyncService>,
    connection_workspace: Entity<ConnectionWorkspaceState>,
    _connection_workspace_observation: Subscription,
    _connection_workspace_subscription: Subscription,
    remote_desktop: Entity<remote_desktop::RemoteDesktopWorkspaceEntity>,
    remote_desktop_resize_menu_tab_id: Option<TabId>,
    // Shell discovery spawns helper processes, so the scan is deferred to the
    // first launcher or settings use instead of blocking workspace startup.
    local_shells: RefCell<Vec<ShellInfo>>,
    #[allow(dead_code)]
    local_shells_scanned: Cell<bool>,
    terminal: Entity<WorkspaceTerminalEntity>,
    _terminal_subscription: Subscription,
    overlay: Entity<WorkspaceOverlayEntity>,
    _overlay_observation: Subscription,
}

impl Drop for WorkspaceApp {
    fn drop(&mut self) {
        // App Lock and Cloud Sync move the focused secret into this window IME
        // adapter, so window destruction must zeroize it even without a blur event.
        zeroize::Zeroize::zeroize(&mut self.settings_input_draft);
        // WorkspaceApp owns the shared session runtime. Window, tab, and page
        // release must not stop transfers or tunnels; final owner drop must.
        self.shutdown_final_session_services();
    }
}

pub(crate) use window_shell::WorkspaceWindowShell;

#[derive(Clone)]
struct MermaidZoomState {
    source: String,
    image: Arc<Image>,
    width: f32,
    height: f32,
}

impl WorkspaceApp {
    fn localized_markdown_options(&self) -> MarkdownOptions {
        let mut options = MarkdownOptions::from_theme(&self.tokens);
        options.mermaid_error_prefix = self.i18n.t("markdown.mermaid_unsupported");
        options.mermaid_expand_label = self.i18n.t("markdown.mermaid_expand");
        options
    }

    fn mermaid_zoom_handler(&self, cx: &mut Context<Self>) -> MarkdownMermaidZoomHandler {
        let workspace = cx.entity();
        Arc::new(move |source, image, width, height, window, cx| {
            let workspace = workspace.clone();
            window.defer(cx, move |_window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    let rendered = oxideterm_gpui_markdown::mermaid::render_mermaid_svg_scaled(
                        &source,
                        &this.tokens,
                        &this.localized_markdown_options(),
                        MERMAID_MODAL_RASTER_SCALE,
                    )
                    .ok();
                    this.mermaid_zoom = Some(MermaidZoomState {
                        source,
                        image: rendered
                            .as_ref()
                            .map(|rendered| rendered.image.clone())
                            .unwrap_or(image),
                        width: rendered
                            .as_ref()
                            .map(|rendered| rendered.display_width)
                            .unwrap_or(width),
                        height: rendered
                            .as_ref()
                            .map(|rendered| rendered.display_height)
                            .unwrap_or(height),
                    });
                    cx.notify();
                });
            });
        })
    }

    fn markdown_mermaid_actions(&self, cx: &mut Context<Self>) -> MarkdownCodeBlockActions {
        MarkdownCodeBlockActions {
            on_run: None,
            on_mermaid_zoom: Some(self.mermaid_zoom_handler(cx)),
        }
    }
}

// Completion providers remain data-only while the sender editor owns input.
#[allow(dead_code)]
#[derive(Clone, Debug)]
struct TerminalCommandSuggestion {
    kind: TerminalCommandSuggestionKind,
    label: String,
    insert_text: String,
    description: Option<String>,
    executable: bool,
    replacement: std::ops::Range<usize>,
    group_label_key: &'static str,
    source_label_key: &'static str,
    score: f64,
    risk: Option<&'static str>,
    inline_safe: bool,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalCommandSuggestionKind {
    History,
    Command,
    Subcommand,
    Option,
    File,
    Directory,
    QuickCommand,
}

#[cfg(test)]
thread_local! {
    static FAIL_NEXT_SESSION_TREE_REPLACE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
