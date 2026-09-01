use super::*;
use oxideterm_gpui_ui::button::ButtonRadius;
use oxideterm_gpui_ui::{IconButtonOptions, TreeBranchMetrics, tree_child};

// Active sessions are a high-frequency navigator, so keep its density closer
// to a compact desktop connection list than to a settings or form surface.
// Values are deliberately roomier (40% above the original compact spacing)
// so session cards, icons, and labels stay readable in the active sessions
// sidebar without turning every row into a dense two-line collision.
const SESSION_TREE_NODE_HEIGHT: f32 = 40.0;
const SESSION_TREE_ITEM_HEIGHT: f32 = 34.0;
const SESSION_TREE_TEXT_SIZE: f32 = 13.0;
const SESSION_TREE_META_TEXT_SIZE: f32 = 11.0;
const SESSION_TREE_ICON_SIZE: f32 = 19.0;
const SESSION_TREE_CHILD_ICON_SIZE: f32 = 16.0;
// Primary sidebar content needs a small inset below the header divider so the
// first interactive surface does not visually merge with workspace chrome.
const PRIMARY_SIDEBAR_CONTENT_TOP_INSET: f32 = 4.0;
// Tauri FocusedNodeList uses accent/emerald alpha utility classes such as
// `bg-oxide-accent/5`, `border-oxide-accent/50`, and `bg-emerald-500/20`.
// Keep the translated alpha roles named so this card view does not drift into
// feature-local magic colors.
const SESSION_FOCUS_CARD_SELECTED_BG_ALPHA: u32 = 0x0d;
const SESSION_FOCUS_CARD_SELECTED_BORDER_ALPHA: u32 = 0x80;
const SESSION_FOCUS_CARD_BORDER_ALPHA: u32 = 0x80;
const SESSION_FOCUS_TERMINAL_BADGE_BG_ALPHA: u32 = 0x33;
const SESSION_FOCUS_TERMINAL_BADGE_HOVER_ALPHA: u32 = 0x4d;
const SESSION_FOCUS_ACTION_BG_ALPHA: u32 = 0x1a;
const SESSION_FOCUS_DIVIDER_ALPHA: u32 = 0x4d;
// Active-session list rows share one thin bottom separator so connected,
// connecting, and saved-bookmark rows read as one coherent list instead of
// floating cards with invisible boundaries.
const SESSION_ROW_SEPARATOR_ALPHA: u32 = 0x26;
// Tauri FocusedNodeList empty state uses `w-8 h-8 opacity-30`,
// `text-sm`, `text-xs`, and `opacity-60` for the helper text.
const SESSION_FOCUS_EMPTY_ICON_SIZE: f32 = 32.0;
const SESSION_FOCUS_EMPTY_ICON_ALPHA: u32 = 0x4d;
const SESSION_FOCUS_EMPTY_TITLE_TEXT_SIZE: f32 = 14.0;
const SESSION_FOCUS_EMPTY_SUBTITLE_TEXT_SIZE: f32 = 12.0;
const SESSION_FOCUS_EMPTY_SUBTITLE_ALPHA: f32 = 0.6;
const SESSION_FOCUS_EMERALD: u32 = 0x10b981;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SidebarSection {
    Sessions,
    Connections,
    HostTools,
    Automation,
    Workspace,
    Monitor,
    Settings,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ContextSidebarPanel {
    HostTools,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ContextSidebarTool {
    /// Remote file browser. Host Tools opens on this surface by default so
    /// the first thing an operator sees is the connected host's filesystem.
    Files,
    Monitor,
    Gpu,
    Processes,
    Services,
    Logs,
    Tmux,
    Docker,
    Ports,
    Schedules,
    Filesystems,
    Packages,
}

#[derive(Clone, Copy)]
pub(in crate::workspace) struct SessionStatusStyle {
    icon: LucideIcon,
    text_color: u32,
    dot_color: u32,
    opacity: f32,
    ring: bool,
}

#[derive(Clone, Copy)]
pub(in crate::workspace) enum SessionActionVariant {
    Primary,
    Danger,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::workspace) enum ActiveSessionSidebarViewMode {
    Tree,
}

#[derive(Clone, Debug)]
pub(in crate::workspace) struct ActiveSessionContextMenu {
    pub saved_connection_id: Option<String>,
    pub node_id: Option<NodeId>,
    pub title: String,
    pub group: Option<String>,
    pub profile_kind: Option<sessions::PendingSessionProfileKind>,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug)]
pub(in crate::workspace) struct MoveSessionFolderDialogState {
    pub connection_id: String,
    pub connection_title: String,
    pub current_group: Option<String>,
    pub selected_group: Option<String>,
    pub custom_folder_name: String,
    pub is_custom: bool,
    pub profile_kind: Option<sessions::PendingSessionProfileKind>,
}

#[derive(Clone, Debug)]
pub(in crate::workspace) struct ActiveSessionFolderContextMenu {
    pub group: String,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, Default)]
pub(in crate::workspace) struct NewSessionFolderDialogState {
    pub folder_name: String,
    /// When set, the dialog renames this existing group instead of creating
    /// a new top-level folder. The confirm action must call rename_group and
    /// keep the same IME/input lifecycle as the create path.
    pub rename_group: Option<String>,
}

impl SidebarSection {
    pub(super) fn from_settings_key(key: &str) -> Self {
        match key {
            // The retired saved-connections sidebar now restores the active
            // session navigator while the full manager remains a workspace tab.
            "connections" | "saved" => Self::Sessions,
            // Embedded SFTP now shares the active-sessions panel. Preserve the
            // old persisted key as a migration path instead of reopening a
            // retired standalone sidebar.
            "sftp" => Self::Sessions,
            // Retired dashboard and notification-center keys restore the active
            // session navigator, matching the retired saved-connections keys.
            "runtime" | "connection_pool" | "terminal" | "network" | "topology"
            | "notifications" => Self::Sessions,
            // Retired health-page keys now restore the Host Tools replacement.
            "connection_monitor" | "activity" => Self::HostTools,
            "host_tools" => Self::HostTools,
            "automation" => Self::Automation,
            "workspace" => Self::Workspace,
            // The local file manager tab was removed; restore the retired
            // "files" key to the active session navigator like other retirees.
            "files" => Self::Sessions,
            "monitor" => Self::Monitor,
            "settings" => Self::Settings,
            _ => Self::Sessions,
        }
    }

    pub(super) fn as_settings_key(self) -> &'static str {
        match self {
            Self::Sessions => "sessions",
            // Keep the activity identity stable; persistence stores the
            // effective Sessions panel instead of this tab-only entry.
            Self::Connections => "saved",
            Self::HostTools => "host_tools",
            Self::Automation => "automation",
            Self::Workspace => "workspace",
            Self::Monitor => "monitor",
            Self::Settings => "settings",
        }
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn effective_sidebar_panel_section(&self) -> SidebarSection {
        match self.active_sidebar_section {
            SidebarSection::Sessions => self.active_sidebar_section,
            // Tauri separates activity-bar tab buttons from sidebar sections.
            // Keep tab-only entries from replacing the Sessions sidebar body.
            SidebarSection::Connections
            | SidebarSection::HostTools
            | SidebarSection::Automation
            | SidebarSection::Workspace
            | SidebarSection::Monitor
            | SidebarSection::Settings => SidebarSection::Sessions,
        }
    }
}

mod activity;
mod helpers;
mod region;
mod sessions;
mod state;
mod titlebar;

use helpers::*;
pub(in crate::workspace) use sessions::ActiveSessionSidebarRow;
pub(in crate::workspace) use state::{
    clamp_responsive_sidebar_width, context_sidebar_panel_visible,
};

#[cfg(test)]
mod sidebar_persistence_tests {
    use super::SidebarSection;

    #[test]
    fn sidebar_sections_roundtrip_persisted_settings_keys() {
        let sections = [
            SidebarSection::Sessions,
            SidebarSection::HostTools,
            SidebarSection::Automation,
            SidebarSection::Workspace,
            SidebarSection::Monitor,
            SidebarSection::Settings,
        ];

        for section in sections {
            assert_eq!(
                SidebarSection::from_settings_key(section.as_settings_key()),
                section
            );
        }
    }

    #[test]
    fn retired_sidebar_keys_restore_active_sessions() {
        for key in [
            "connections",
            "saved",
            "sftp",
            "runtime",
            "connection_pool",
            "terminal",
            "network",
            "topology",
            "notifications",
            // The local file manager activity-bar entry was removed.
            "files",
        ] {
            assert_eq!(
                SidebarSection::from_settings_key(key),
                SidebarSection::Sessions
            );
        }
    }
}
