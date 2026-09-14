use std::{
    collections::{HashMap, HashSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
};

use super::*;
use crate::workspace::{
    browser_behavior,
    session_icons::{default_connection_transport_icon, session_icon_from_id},
};
use gpui::{
    App, Context, CursorStyle, FontWeight, MouseButton, MouseDownEvent,
    MouseMoveEvent, ParentElement, Styled, Window, div, prelude::*, px, rgb, rgba,
};
use oxideterm_connections::{ConnectionTransport, SavedConnection};
use oxideterm_gpui_terminal::TerminalNoticeVariant;
use oxideterm_gpui_ui::{
    button::ButtonTone,
    context_menu::{
        ContextMenuItemKind, context_menu_content, context_menu_event_boundary,
        context_menu_item, context_menu_separator,
    },
    modal::{dismissible_dialog_backdrop, overlay_content_boundary},
    text_input::{TextInputView, text_input, text_input_anchor_probe},
};
use oxideterm_remote_desktop::{RemoteDesktopProtocol, RemoteDesktopSessionStatus};
use oxideterm_terminal::TerminalSessionKind;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum StandaloneActiveSessionKind {
    Telnet,
    Serial,
    Rdp,
    Vnc,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum StandaloneActiveSessionTarget {
    Terminal(TerminalSessionId),
    RemoteDesktop(TabId),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct StandaloneActiveSession {
    kind: StandaloneActiveSessionKind,
    target: StandaloneActiveSessionTarget,
}

/// Pending saved assets use distinct stores and launchers; keeping their kind
/// on the row prevents the SSH-only connection flow from handling them.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::workspace) enum PendingSessionProfileKind {
    Ssh,
    Telnet,
    Serial,
    Rdp,
    Vnc,
}

#[derive(Clone)]
pub(in crate::workspace) struct ActiveSessionSidebarRow {
    node_id: NodeId,
    parent_id: Option<NodeId>,
    saved_connection_id: Option<String>,
    title: String,
    host: String,
    username: String,
    port: u16,
    group: Option<String>,
    node_view: ActiveSessionNode,
    depth: usize,
    #[allow(dead_code)]
    is_last: bool,
    #[allow(dead_code)]
    has_children: bool,
    standalone_session: Option<StandaloneActiveSession>,
    pending_profile: Option<PendingSessionProfileKind>,
    /// A folder header row groups child saved-connection rows; clicking it
    /// toggles the child rows instead of opening a connection.
    is_folder: bool,
    /// Child rows carry their folder's node id so the virtual list can keep
    /// rows collapsed when the folder node is absent from the rows cache.
    parent_is_folder: bool,
}

#[derive(Clone, Copy)]
struct RuntimeRowClaimIdentity<'a> {
    saved_connection_id: Option<&'a str>,
    host: &'a str,
    username: &'a str,
    port: u16,
}

fn runtime_row_claim_index(
    runtime_rows: &[RuntimeRowClaimIdentity<'_>],
    consumed: &[bool],
    connection: &SavedConnection,
) -> Option<usize> {
    runtime_rows
        .iter()
        .enumerate()
        .find(|(index, row)| {
            !consumed[*index] && row.saved_connection_id == Some(connection.id.as_str())
        })
        .map(|(index, _)| index)
        .or_else(|| {
            runtime_rows
                .iter()
                .enumerate()
                .find(|(index, row)| {
                    !consumed[*index]
                        // Endpoint fallback is only for ad-hoc runtime nodes. A node
                        // owned by another saved id cannot lend its display identity.
                        && row.saved_connection_id.is_none()
                        && !row.host.is_empty()
                        && row.host == connection.host
                        && row.port == connection.port
                        && row.username == connection.username
                })
                .map(|(index, _)| index)
        })
}


fn leftover_runtime_row_is_duplicate(
    depth: usize,
    saved_connection_id: Option<&str>,
    host: &str,
    port: u16,
    username: &str,
    claimed_endpoints: &HashSet<(String, u16, String)>,
) -> bool {
    depth == 0
        && saved_connection_id.is_none()
        && !host.is_empty()
        && claimed_endpoints.contains(&(host.to_string(), port, username.to_string()))
}

fn standalone_terminal_kind(kind: TerminalSessionKind) -> Option<StandaloneActiveSessionKind> {
    match kind {
        TerminalSessionKind::Telnet => Some(StandaloneActiveSessionKind::Telnet),
        TerminalSessionKind::Serial => Some(StandaloneActiveSessionKind::Serial),
        TerminalSessionKind::LocalPty | TerminalSessionKind::SshPty => None,
    }
}

fn standalone_remote_desktop_kind(protocol: RemoteDesktopProtocol) -> StandaloneActiveSessionKind {
    match protocol {
        RemoteDesktopProtocol::Rdp => StandaloneActiveSessionKind::Rdp,
        RemoteDesktopProtocol::Vnc => StandaloneActiveSessionKind::Vnc,
    }
}

fn standalone_session_icon(kind: StandaloneActiveSessionKind) -> LucideIcon {
    let transport = match kind {
        StandaloneActiveSessionKind::Telnet => ConnectionTransport::Telnet,
        StandaloneActiveSessionKind::Serial => ConnectionTransport::Serial,
        StandaloneActiveSessionKind::Rdp => ConnectionTransport::Rdp,
        StandaloneActiveSessionKind::Vnc => ConnectionTransport::Vnc,
    };
    default_connection_transport_icon(transport)
}

fn pending_profile_icon(kind: PendingSessionProfileKind) -> LucideIcon {
    let transport = match kind {
        PendingSessionProfileKind::Ssh => ConnectionTransport::Ssh,
        PendingSessionProfileKind::Telnet => ConnectionTransport::Telnet,
        PendingSessionProfileKind::Serial => ConnectionTransport::Serial,
        PendingSessionProfileKind::Rdp => ConnectionTransport::Rdp,
        PendingSessionProfileKind::Vnc => ConnectionTransport::Vnc,
    };
    default_connection_transport_icon(transport)
}

fn terminal_lifecycle_readiness(lifecycle: &TerminalLifecycle) -> ActiveSessionReadiness {
    match lifecycle {
        TerminalLifecycle::Running => ActiveSessionReadiness::Ready,
        TerminalLifecycle::Exited(_) => ActiveSessionReadiness::Error,
        TerminalLifecycle::Closed => ActiveSessionReadiness::Disconnected,
    }
}

fn remote_desktop_readiness(status: RemoteDesktopSessionStatus) -> ActiveSessionReadiness {
    match status {
        RemoteDesktopSessionStatus::Connected => ActiveSessionReadiness::Ready,
        RemoteDesktopSessionStatus::Connecting | RemoteDesktopSessionStatus::Reconnecting => {
            ActiveSessionReadiness::Connecting
        }
        RemoteDesktopSessionStatus::Failed => ActiveSessionReadiness::Error,
        RemoteDesktopSessionStatus::Idle | RemoteDesktopSessionStatus::Disconnected => {
            ActiveSessionReadiness::Disconnected
        }
    }
}

fn standalone_session_click_should_focus(click_count: usize) -> bool {
    click_count >= 2
}

fn active_session_readiness(readiness: &NodeReadiness) -> ActiveSessionReadiness {
    match readiness {
        NodeReadiness::Ready => ActiveSessionReadiness::Ready,
        NodeReadiness::Connecting => ActiveSessionReadiness::Connecting,
        NodeReadiness::Error => ActiveSessionReadiness::Error,
        NodeReadiness::Disconnected => ActiveSessionReadiness::Disconnected,
    }
}

#[derive(Clone, Copy, Debug)]
pub(in crate::workspace) struct SessionStatusStyle {
    pub icon: LucideIcon,
    #[allow(dead_code)]
    pub text_color: u32,
    #[allow(dead_code)]
    pub dot_color: u32,
    #[allow(dead_code)]
    pub opacity: f32,
    #[allow(dead_code)]
    pub ring: bool,
}

impl WorkspaceApp {
    fn saved_profile_icon(
        &self,
        profile_id: &str,
        profile_kind: PendingSessionProfileKind,
    ) -> LucideIcon {
        let stored_icon = match profile_kind {
            PendingSessionProfileKind::Ssh => self
                .connection_store
                .get(profile_id)
                .and_then(|profile| profile.icon.as_deref()),
            PendingSessionProfileKind::Telnet => self
                .connection_store
                .telnet_profiles()
                .iter()
                .find(|profile| profile.id == profile_id)
                .and_then(|profile| profile.icon.as_deref()),
            PendingSessionProfileKind::Serial => self
                .connection_store
                .serial_profiles()
                .iter()
                .find(|profile| profile.id == profile_id)
                .and_then(|profile| profile.icon.as_deref()),
            PendingSessionProfileKind::Rdp | PendingSessionProfileKind::Vnc => self
                .connection_store
                .remote_desktop_profiles()
                .iter()
                .find(|profile| profile.id == profile_id)
                .and_then(|profile| profile.icon.as_deref()),
        };
        stored_icon
            .and_then(|icon| session_icon_from_id(Some(icon)))
            .unwrap_or_else(|| pending_profile_icon(profile_kind))
    }

    pub(in crate::workspace) fn session_node_status(
        &self,
        status: ActiveSessionStatus,
    ) -> SessionStatusStyle {
        let theme = self.tokens.ui;
        match status {
            ActiveSessionStatus::Active | ActiveSessionStatus::Connected => SessionStatusStyle {
                icon: LucideIcon::Server,
                text_color: theme.text,
                dot_color: theme.accent,
                opacity: 1.0,
                ring: false,
            },
            ActiveSessionStatus::Connecting => SessionStatusStyle {
                icon: LucideIcon::LoaderCircle,
                text_color: theme.text,
                dot_color: theme.accent,
                opacity: 1.0,
                ring: true,
            },
            ActiveSessionStatus::Error => SessionStatusStyle {
                icon: LucideIcon::Server,
                text_color: theme.text,
                dot_color: theme.error,
                opacity: 1.0,
                ring: false,
            },
            ActiveSessionStatus::Idle => SessionStatusStyle {
                icon: LucideIcon::Server,
                text_color: theme.text,
                dot_color: theme.text_muted,
                opacity: 1.0,
                ring: false,
            },
        }
    }

    pub(in crate::workspace) fn render_active_sessions_sidebar_content(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = self.active_session_sidebar_rows(cx);
        if rows.is_empty() {
            return self.render_empty_sessions_sidebar_content(cx);
        }

        self.sync_active_session_sidebar_list_state(&rows, cx);
        let state = self.active_session_sidebar_list_state.clone();
        let spec = self.active_session_sidebar_list_spec();
        let workspace = cx.entity();
        div()
            .id("active-sessions-sidebar-scroll")
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .pt(px(PRIMARY_SIDEBAR_CONTENT_TOP_INSET))
            .child(tauri_virtual_list(
                state,
                spec,
                move |index, _window, cx| {
                    workspace.update(cx, |this, cx| {
                        this.render_active_session_sidebar_list_item(index, cx)
                    })
                },
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn active_session_sidebar_rows(
        &self,
        cx: &App,
    ) -> Vec<ActiveSessionSidebarRow> {
        // The saved-connection catalog owns row placement. Runtime nodes render
        // at their catalog position, so connecting never jumps a row to the
        // top; runtime-only nodes (quick connect, drill-down children) keep
        // their tree order in a tail section after the catalog entries.
        let runtime_rows: Vec<ActiveSessionSidebarRow> = {
            let mut tree_nodes = self.node_router.flatten_tree();
            let flat_node_child_counts = tree_nodes
                .iter()
                .filter_map(|node| node.parent_id.as_ref())
                .fold(HashMap::<String, usize>::new(), |mut counts, parent_id| {
                    *counts.entry(parent_id.clone()).or_default() += 1;
                    counts
                });
            tree_nodes
                .drain(..)
                .filter_map(|flat_node| {
                    let flat_node_id = flat_node.id.clone();
                    let node_id = NodeId::new(flat_node_id.clone());
                    let node = self.ssh_nodes.get(&node_id)?.clone();
                    let node_view = ActiveSessionNode {
                        id: flat_node_id.clone(),
                        title: node.title.clone(),
                        port: flat_node.port,
                        terminal_ids: node.terminal_ids.clone(),
                        readiness: active_session_readiness(&node.readiness),
                    };
                    Some(ActiveSessionSidebarRow {
                        node_id,
                        parent_id: flat_node.parent_id.map(NodeId::new),
                        saved_connection_id: node.saved_connection_id.clone(),
                        title: node.title.clone(),
                        host: node.endpoint.host.clone(),
                        username: node.endpoint.username.clone(),
                        port: node.endpoint.port,
                        group: None,
                        node_view,
                        depth: flat_node.depth as usize,
                        is_last: flat_node.is_last_child,
                        has_children: flat_node_child_counts
                            .get(&flat_node_id)
                            .is_some_and(|count| *count > 0),
                        standalone_session: None,
                        pending_profile: None,
                        is_folder: false,
                        parent_is_folder: false,
                    })
                })
                .collect()
        };
        // A catalog entry claims its runtime node by saved id first. Endpoint
        // fallback remains available only for runtime nodes without a saved owner.
        let mut consumed = vec![false; runtime_rows.len()];
        let runtime_claim_identities: Vec<_> = runtime_rows
            .iter()
            .map(|row| RuntimeRowClaimIdentity {
                saved_connection_id: row.saved_connection_id.as_deref(),
                host: &row.host,
                username: &row.username,
                port: row.port,
            })
            .collect();
        let mut claim_runtime_row = |connection: &SavedConnection| -> Option<ActiveSessionSidebarRow> {
            let index =
                runtime_row_claim_index(&runtime_claim_identities, &consumed, connection)?;
            consumed[index] = true;
            Some(runtime_rows[index].clone())
        };

        // Standalone sessions are built before the catalog pass: a running
        // Telnet/Serial/RDP/VNC session claims its saved profile's row, so the
        // sidebar never shows the same asset twice (pending + running).
        let standalone_rows: Vec<ActiveSessionSidebarRow> = self
            .tabs(cx)
            .iter()
            .filter_map(|tab| self.standalone_active_session_sidebar_row(tab, cx))
            .collect();
        let mut standalone_by_profile: HashMap<String, ActiveSessionSidebarRow> = HashMap::new();
        for row in standalone_rows.iter() {
            if let Some(profile_id) = row.saved_connection_id.clone() {
                standalone_by_profile.insert(profile_id, row.clone());
            }
        }

        // First pass: every catalog connection claims its runtime node (or becomes
// a pending row) regardless of folder expansion, so a collapsed folder
// never leaks its connected nodes into the runtime tail.
        let mut claimed: HashMap<Option<String>, Vec<ActiveSessionSidebarRow>> = HashMap::new();
        for connection in self.connection_store.connections() {
            let is_folder_child = connection.group.is_some();
            let row = match claim_runtime_row(connection) {
                Some(mut node_row) => {
                    node_row.depth = if is_folder_child { 1 } else { 0 };
                    node_row.parent_is_folder = is_folder_child;
                    // Catalog display identity is owned by the saved record, not
                    // by whichever runtime node happened to match an endpoint.
                    node_row.title = connection.name.clone();
                    node_row.node_view.title = connection.name.clone();
                    node_row
                }
                None => Self::pending_connection_row(connection),
            };
            claimed.entry(connection.group.clone()).or_default().push(row);
        }
        for profile in self.connection_store.telnet_profiles() {
            let is_folder_child = profile.group.is_some();
            let mut row = match standalone_by_profile.remove(&profile.id) {
                Some(mut running_row) => {
                    // The running row inherits the catalog identity and endpoint.
                    running_row.title = profile.name.clone();
                    running_row.node_view.title = profile.name.clone();
                    running_row.host = profile.host.clone();
                    running_row.port = profile.port;
                    running_row
                }
                None => Self::pending_telnet_profile_row(profile),
            };
            row.depth = if is_folder_child { 1 } else { 0 };
            row.parent_is_folder = is_folder_child;
            claimed.entry(profile.group.clone()).or_default().push(row);
        }
        for profile in self.connection_store.serial_profiles() {
            let is_folder_child = profile.group.is_some();
            let mut row = match standalone_by_profile.remove(&profile.id) {
                Some(mut running_row) => {
                    running_row.title = profile.name.clone();
                    running_row.node_view.title = profile.name.clone();
                    running_row.host = profile.port_path.clone();
                    running_row
                }
                None => Self::pending_serial_profile_row(profile),
            };
            row.depth = if is_folder_child { 1 } else { 0 };
            row.parent_is_folder = is_folder_child;
            claimed.entry(profile.group.clone()).or_default().push(row);
        }
        for profile in self.connection_store.remote_desktop_profiles() {
            let is_folder_child = profile.group.is_some();
            let mut row = match standalone_by_profile.remove(&profile.id) {
                Some(mut running_row) => {
                    running_row.title = profile.name.clone();
                    running_row.node_view.title = profile.name.clone();
                    running_row.host = profile.host.clone();
                    running_row.port = profile.port;
                    running_row
                }
                None => Self::pending_remote_desktop_profile_row(profile),
            };
            row.depth = if is_folder_child { 1 } else { 0 };
            row.parent_is_folder = is_folder_child;
            claimed.entry(profile.group.clone()).or_default().push(row);
        }

        let mut rows = Vec::new();
        // Folder rows render for every configured group so an empty folder is
        // still visible after creation; collapsing works through the same
        // expanded-node set used by SSH subtree drilling.
        let configured_groups = self.connection_store.groups().to_vec();
        for group in configured_groups {
            let folder_node_id = NodeId::new(format!("folder-{group}"));
            let group_rows = claimed.remove(&Some(group.clone())).unwrap_or_default();
            let expanded = self.expanded_ssh_nodes.contains(&folder_node_id);
            rows.push(ActiveSessionSidebarRow {
                node_id: folder_node_id.clone(),
                parent_id: None,
                saved_connection_id: None,
                title: group.clone(),
                host: String::new(),
                username: String::new(),
                port: 0,
                group: None,
                node_view: ActiveSessionNode {
                    id: format!("folder-{group}"),
                    title: group.clone(),
                    port: 0,
                    terminal_ids: Vec::new(),
                    readiness: ActiveSessionReadiness::Disconnected,
                },
                depth: 0,
                is_last: true,
                has_children: !group_rows.is_empty(),
                standalone_session: None,
                pending_profile: None,
                is_folder: true,
                parent_is_folder: false,
            });
            if expanded {
                rows.extend(group_rows);
            }
        }
        // Ungrouped catalog connections sit at the root of the list.
        rows.extend(claimed.remove(&None).unwrap_or_default());

        // Standalone sessions without a saved profile (ad-hoc launches) keep
        // their own root-level rows; claimed ones already sit in their folder.
        rows.extend(
            standalone_rows
                .into_iter()
                .filter(|row| row.saved_connection_id.is_none()),
        );

        // The registry can hold sibling runtime nodes for one identity (stale
        // index rebuilds, connect-while-prompt flows). One catalog identity
        // renders exactly one row: drop root-level leftovers whose endpoint a
        // claimed catalog row already represents. Drill-down children keep
        // their own endpoints and stay visible.
        let claimed_endpoints: HashSet<(String, u16, String)> = rows
            .iter()
            .filter(|row| row.pending_profile.is_none() && !row.host.is_empty())
            .map(|row| (row.host.clone(), row.port, row.username.clone()))
            .collect();
        for (index, row) in runtime_rows.into_iter().enumerate() {
            if consumed[index] {
                continue;
            }
            if leftover_runtime_row_is_duplicate(
                row.depth,
                row.saved_connection_id.as_deref(),
                &row.host,
                row.port,
                &row.username,
                &claimed_endpoints,
            ) {
                continue;
            }
            rows.push(row);
        }
        rows
    }

    fn pending_connection_row(connection: &SavedConnection) -> ActiveSessionSidebarRow {
        let is_folder_child = connection.group.is_some();
        ActiveSessionSidebarRow {
            node_id: NodeId::new(format!("saved-connection-{}", connection.id)),
            parent_id: None,
            saved_connection_id: Some(connection.id.clone()),
            title: connection.name.clone(),
            host: connection.host.clone(),
            username: connection.username.clone(),
            port: connection.port,
            group: connection.group.clone(),
            node_view: ActiveSessionNode {
                id: format!("saved-connection-{}", connection.id),
                title: connection.name.clone(),
                port: connection.port,
                terminal_ids: Vec::new(),
                readiness: ActiveSessionReadiness::Disconnected,
            },
            depth: if is_folder_child { 1 } else { 0 },
            is_last: true,
            has_children: false,
            standalone_session: None,
            pending_profile: Some(PendingSessionProfileKind::Ssh),
            is_folder: false,
            parent_is_folder: is_folder_child,
        }
    }

    fn pending_telnet_profile_row(
        profile: &oxideterm_connections::TelnetProfile,
    ) -> ActiveSessionSidebarRow {
        Self::pending_profile_row(
            &profile.id,
            &profile.name,
            &profile.host,
            profile.port,
            profile.group.clone(),
            PendingSessionProfileKind::Telnet,
        )
    }

    fn pending_serial_profile_row(
        profile: &oxideterm_connections::SerialProfile,
    ) -> ActiveSessionSidebarRow {
        Self::pending_profile_row(
            &profile.id,
            &profile.name,
            &profile.port_path,
            0,
            profile.group.clone(),
            PendingSessionProfileKind::Serial,
        )
    }

    fn pending_remote_desktop_profile_row(
        profile: &oxideterm_connections::RemoteDesktopProfile,
    ) -> ActiveSessionSidebarRow {
        let kind = match profile.protocol {
            RemoteDesktopProtocol::Rdp => PendingSessionProfileKind::Rdp,
            RemoteDesktopProtocol::Vnc => PendingSessionProfileKind::Vnc,
        };
        Self::pending_profile_row(
            &profile.id,
            &profile.name,
            &profile.host,
            profile.port,
            profile.group.clone(),
            kind,
        )
    }

    fn pending_profile_row(
        id: &str,
        title: &str,
        host: &str,
        port: u16,
        group: Option<String>,
        kind: PendingSessionProfileKind,
    ) -> ActiveSessionSidebarRow {
        ActiveSessionSidebarRow {
            node_id: NodeId::new(format!("pending-profile-{id}")),
            parent_id: None,
            saved_connection_id: Some(id.to_string()),
            title: title.to_string(),
            host: host.to_string(),
            username: String::new(),
            port,
            group,
            node_view: ActiveSessionNode {
                id: format!("pending-profile-{id}"),
                title: title.to_string(),
                port,
                terminal_ids: Vec::new(),
                readiness: ActiveSessionReadiness::Disconnected,
            },
            depth: 0,
            is_last: true,
            has_children: false,
            standalone_session: None,
            pending_profile: Some(kind),
            is_folder: false,
            parent_is_folder: false,
        }
    }

    fn standalone_active_session_sidebar_row(
        &self,
        tab: &Tab,
        cx: &App,
    ) -> Option<ActiveSessionSidebarRow> {
        let (standalone_session, readiness, terminal_ids, row_id) =
            if tab.kind == TabKind::RemoteDesktop {
                let session = self.remote_desktop.read(cx).session(tab.id)?;
                let session = session.read(cx);
                let protocol = session.active_session_protocol();
                (
                    StandaloneActiveSession {
                        kind: standalone_remote_desktop_kind(protocol),
                        target: StandaloneActiveSessionTarget::RemoteDesktop(tab.id),
                    },
                    remote_desktop_readiness(session.active_session_status()),
                    Vec::new(),
                    format!("remote-desktop-session-{}", tab.id.0),
                )
            } else {
                let session_id = tab.active_pane_id.and_then(|pane_id| {
                    tab.root_pane
                        .as_ref()
                        .and_then(|root| root.session_id_for_pane(pane_id))
                })?;
                let shared_session = self
                    .tab_host
                    .read(cx)
                    .terminal_location(session_id)
                    .and_then(|location| {
                        self.tab_host
                            .read(cx)
                            .panes()
                            .get(&location.pane_id)
                            .map(|pane| pane.read(cx).shared_session())
                    })?;
                let terminal = shared_session.lock();
                let kind = standalone_terminal_kind(terminal.kind())?;
                let readiness = terminal_lifecycle_readiness(&terminal.lifecycle());
                (
                    StandaloneActiveSession {
                        kind,
                        target: StandaloneActiveSessionTarget::Terminal(session_id),
                    },
                    readiness,
                    vec![session_id],
                    format!("standalone-terminal-session-{}", session_id.0),
                )
            };

        // A session launched from a saved profile keeps the profile id so the
        // running row claims the pending catalog row instead of duplicating it.
        let saved_profile_id = if tab.kind == TabKind::RemoteDesktop {
            self.remote_desktop
                .read(cx)
                .session(tab.id)
                .map(|session| session.read(cx).saved_profile_id().to_string())
        } else {
            terminal_ids.first().and_then(|session_id| {
                self.telnet_terminal_profile_ids
                    .get(session_id)
                    .or_else(|| self.serial_terminal_profile_ids.get(session_id))
                    .cloned()
            })
        };

        let node_id = NodeId::new(row_id.clone());
        Some(ActiveSessionSidebarRow {
            node_id,
            parent_id: None,
            saved_connection_id: saved_profile_id,
            title: tab.title.clone(),
            host: String::new(),
            username: String::new(),
            port: 0,
            group: None,
            node_view: ActiveSessionNode {
                id: row_id,
                title: tab.title.clone(),
                port: 0,
                terminal_ids,
                readiness,
            },
            depth: 0,
            is_last: true,
            has_children: false,
            standalone_session: Some(standalone_session),
            pending_profile: None,
            is_folder: false,
            parent_is_folder: false,
        })
    }

    pub(in crate::workspace) fn sync_active_session_sidebar_list_state(
        &mut self,
        rows: &[ActiveSessionSidebarRow],
        cx: &App,
    ) {
        let signatures = rows
            .iter()
            .map(|row| self.active_session_sidebar_row_signature(row, cx))
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.active_session_sidebar_list_state,
            &mut self.active_session_sidebar_list_cache.borrow_mut(),
            "active-sessions-sidebar",
            &signatures,
            self.active_session_sidebar_list_spec(),
        );
        self.active_session_sidebar_rows_cache
            .replace(Some((ActiveSessionSidebarViewMode::Tree, rows.to_vec())));
    }

    pub(in crate::workspace) fn active_session_sidebar_list_spec(
        &self,
    ) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(ACTIVE_SESSION_SIDEBAR_LIST_ESTIMATED_HEIGHT),
            ACTIVE_SESSION_SIDEBAR_LIST_OVERSCAN,
        )
    }

    pub(in crate::workspace) fn render_active_session_sidebar_list_item(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(row) = self
            .active_session_sidebar_rows_cache
            .borrow()
            .as_ref()
            .and_then(|(_view_mode, rows)| rows.get(index).cloned())
        else {
            return div().into_any_element();
        };
        div()
            // The virtual-list wrapper owns the stable interactive identity;
            // child row shapes can change between pending and connected states.
            .id(("active-session-row", index))
            .px_1()
            .child(self.render_active_session_node(row, cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn active_session_sidebar_row_signature(
        &self,
        row: &ActiveSessionSidebarRow,
        cx: &App,
    ) -> u64 {
        let mut hasher = DefaultHasher::new();
        row.node_id.hash(&mut hasher);
        row.parent_id.hash(&mut hasher);
        row.title.hash(&mut hasher);
        row.host.hash(&mut hasher);
        row.depth.hash(&mut hasher);
        row.standalone_session.hash(&mut hasher);
        row.pending_profile.hash(&mut hasher);
        row.group.hash(&mut hasher);
        row.is_folder.hash(&mut hasher);
        row.parent_is_folder.hash(&mut hasher);
        (self.active_ssh_node_id.as_ref() == Some(&row.node_id)).hash(&mut hasher);
        self.has_active_reconnect_job(&row.node_id, cx).hash(&mut hasher);
        // Folder rows re-render with their child rows on expand/collapse.
        if row.is_folder {
            self.expanded_ssh_nodes
                .contains(&row.node_id)
                .hash(&mut hasher);
        }
        hasher.finish()
    }

    pub(in crate::workspace) fn render_active_session_node(
        &self,
        row: ActiveSessionSidebarRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Folder children keep a guide indent under their folder header so the
        // grouping is visible without a second tree gutter column.
        let indent = px(if row.parent_is_folder { 16.0 } else { 0.0 });
        if row.is_folder {
            return self.render_folder_sidebar_row(row, cx);
        }
        if row.standalone_session.is_some() {
            return self.render_standalone_session_sidebar_row(row, cx);
        }
        if row.pending_profile.is_some() {
            return div()
                .pl(indent)
                .child(self.render_pending_saved_connection_row(row, cx))
                .into_any_element();
        }
        let node_id = row.node_id;
        let node_view = row.node_view;
        let selected = self.active_ssh_node_id.as_ref() == Some(&node_id);
        let status = self.session_node_status(node_view.status());
        div()
            .pl(indent)
            .child(self.render_session_node_header(
                node_id,
                node_view,
                &row.host,
                row.saved_connection_id,
                selected,
                status,
                cx,
            ))
            .into_any_element()
    }

    fn render_folder_sidebar_row(
        &self,
        row: ActiveSessionSidebarRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let node_id = row.node_id.clone();
        let expanded = self.expanded_ssh_nodes.contains(&node_id);
        let title = row.title.clone();
        let folder_group = title.clone();

        div()
            .id(format!("active-session-folder-{}", node_id.0))
            .relative()
            .h(px(SESSION_TREE_NODE_HEIGHT))
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .rounded(px(self.tokens.radii.md))
            .px_2()
            .cursor_pointer()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | SESSION_ROW_SEPARATOR_ALPHA))
            .hover(move |row| row.bg(rgb(theme.bg_hover)))
            .child(
                div()
                    .mr(px(8.0))
                    .flex_none()
                    .child(Self::render_lucide_icon(
                        if expanded {
                            LucideIcon::FolderOpen
                        } else {
                            LucideIcon::Folder
                        },
                        SESSION_TREE_ICON_SIZE,
                        rgb(theme.text_muted),
                    )),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .truncate()
                    .text_size(px(SESSION_TREE_TEXT_SIZE))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .child(title),
            )
            .child(
                div()
                    .ml_2()
                    .flex_none()
                    .text_size(px(SESSION_TREE_META_TEXT_SIZE))
                    .text_color(rgb(theme.text_muted))
                    .child(if expanded { "∨" } else { "›" }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if !this.expanded_ssh_nodes.insert(node_id.clone()) {
                        this.expanded_ssh_nodes.remove(&node_id);
                    }
                    this.active_session_sidebar_rows_cache.borrow_mut().take();
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener({
                    let group = folder_group;
                    move |this, event: &MouseDownEvent, _window, cx| {
                        this.open_active_session_folder_context_menu(
                            group.clone(),
                            f32::from(event.position.x),
                            f32::from(event.position.y),
                            cx,
                        );
                        cx.stop_propagation();
                    }
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_session_node_header(
        &self,
        node_id: NodeId,
        node: ActiveSessionNode,
        host: &str,
        saved_connection_id: Option<String>,
        selected: bool,
        status: SessionStatusStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let selected_bg = rgba((theme.accent << 8) | 0x1a);
        let selected_border = rgba((theme.accent << 8) | 0x4d);
        let muted_text = rgb(theme.text_muted);

        let title_str = node.title.clone();
        let host_str = host.to_string();

        div()
            .id(format!("active-session-node-{}", node_id.0))
            .relative()
            .h(px(SESSION_TREE_NODE_HEIGHT))
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .rounded(px(self.tokens.radii.md))
            .px_2()
            .cursor_pointer()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | SESSION_ROW_SEPARATOR_ALPHA))
            .bg(if selected {
                selected_bg
            } else {
                rgba(theme.bg << 8)
            })
            .border_1()
            .border_color(if selected {
                selected_border
            } else {
                rgba(theme.bg << 8)
            })
            .hover(move |row| row.bg(rgb(theme.bg_hover)))
            .child(
                div()
                    .mr(px(8.0))
                    .flex_none()
                    .child(
                        if matches!(status.icon, LucideIcon::LoaderCircle) {
                            self.render_loading_icon(
                                (
                                    gpui::SharedString::from(format!("session-connecting-{node_id:?}")),
                                    0usize,
                                ),
                                SESSION_TREE_ICON_SIZE,
                                rgb(theme.text_muted),
                            )
                        } else {
                            Self::render_lucide_icon(
                                LucideIcon::Server,
                                SESSION_TREE_ICON_SIZE,
                                rgb(theme.text_muted),
                            )
                        },
                    ),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .truncate()
                    .text_size(px(SESSION_TREE_TEXT_SIZE))
                    .font_weight(if selected {
                        gpui::FontWeight::MEDIUM
                    } else {
                        gpui::FontWeight::NORMAL
                    })
                    .text_color(rgb(theme.text))
                    .child(node.title),
            )
            .when(!host.is_empty(), |row| {
                row.child(
                    div()
                        .ml_2()
                        .max_w(px(240.0))
                        .truncate()
                        .text_size(px(SESSION_TREE_META_TEXT_SIZE))
                        .text_color(muted_text)
                        .child(host_str),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let node_id = node_id.clone();
                    let saved_connection_id = saved_connection_id.clone();
                    move |this, event: &MouseDownEvent, window, cx| {
                        // Single click is a deliberate no-op: no selection, no
                        // side effects. Each double click opens one independent
                        // SSH session (a fresh connection per activation).
                        if event.click_count < 2 {
                            cx.stop_propagation();
                            return;
                        }
                        if let Some(id) = saved_connection_id.as_deref() {
                            this.open_saved_connection_new_terminal(id, window, cx);
                        } else {
                            let _ = this.duplicate_ssh_node_connection(&node_id, window, cx);
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener({
                    let saved_connection_id = saved_connection_id.clone();
                    let node_id = node_id.clone();
                    let title = title_str;
                    move |this, event: &MouseDownEvent, _window, cx| {
                        this.open_active_session_context_menu(
                            saved_connection_id.clone(),
                            Some(node_id.clone()),
                            title.clone(),
                            None,
                            None,
                            event.position.x.into(),
                            event.position.y.into(),
                            cx,
                        );
                        cx.stop_propagation();
                    }
                }),
            )
            .into_any_element()
    }

    fn render_pending_saved_connection_row(
        &self,
        row: ActiveSessionSidebarRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(connection_id) = row.saved_connection_id.clone() else {
            return div().into_any_element();
        };
        let muted_text = rgb(theme.text_muted);
        let host = row.host;
        let title_str = row.title.clone();
        let group_str = row.group.clone();
        let profile_kind = row
            .pending_profile
            .expect("pending sidebar rows always carry a profile kind");
        let icon = self.saved_profile_icon(&connection_id, profile_kind);
        let conn_id_clone = connection_id.clone();
        let conn_id_for_right = connection_id.clone();

        div()
            .id(format!("active-session-pending-{connection_id}"))
            .relative()
            .h(px(SESSION_TREE_NODE_HEIGHT))
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .rounded(px(self.tokens.radii.md))
            .px_2()
            .cursor_pointer()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | SESSION_ROW_SEPARATOR_ALPHA))
            .hover(move |surface| surface.bg(rgb(theme.bg_hover)))
            .child(
                div()
                    .mr(px(8.0))
                    .flex_none()
                    .child(Self::render_lucide_icon(
                        icon,
                        SESSION_TREE_ICON_SIZE,
                        rgb(theme.text_muted),
                    )),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .truncate()
                    .text_size(px(SESSION_TREE_TEXT_SIZE))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .text_color(rgb(theme.text))
                    .child(row.title),
            )
            .when(!host.is_empty(), |r| {
                r.child(
                    div()
                        .ml_2()
                        .max_w(px(240.0))
                        .truncate()
                        .text_size(px(SESSION_TREE_META_TEXT_SIZE))
                        .text_color(muted_text)
                        .child(host),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    // Pending saved profiles launch only on double click.
                    if event.click_count >= 2 {
                        match profile_kind {
                            PendingSessionProfileKind::Ssh => {
                                this.open_saved_connection(&conn_id_clone, window, cx);
                            }
                            PendingSessionProfileKind::Telnet => {
                                this.open_saved_telnet_profile(&conn_id_clone, window, cx);
                            }
                            PendingSessionProfileKind::Serial => {
                                this.open_saved_serial_profile(&conn_id_clone, window, cx);
                            }
                            PendingSessionProfileKind::Rdp
                            | PendingSessionProfileKind::Vnc => {
                                this.open_saved_remote_desktop_profile(
                                    &conn_id_clone,
                                    window,
                                    cx,
                                );
                            }
                        }
                    }
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                    this.open_active_session_context_menu(
                        Some(conn_id_for_right.clone()),
                        None,
                        title_str.clone(),
                        group_str.clone(),
                        Some(profile_kind.clone()),
                        event.position.x.into(),
                        event.position.y.into(),
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_standalone_session_sidebar_row(
        &self,
        row: ActiveSessionSidebarRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = row.standalone_session else {
            return div().into_any_element();
        };
        // Running rows claim saved profiles; the menu must edit the same asset.
        let profile_kind = match session.kind {
            StandaloneActiveSessionKind::Telnet => Some(PendingSessionProfileKind::Telnet),
            StandaloneActiveSessionKind::Serial => Some(PendingSessionProfileKind::Serial),
            StandaloneActiveSessionKind::Rdp => Some(PendingSessionProfileKind::Rdp),
            StandaloneActiveSessionKind::Vnc => Some(PendingSessionProfileKind::Vnc),
        };
        let theme = self.tokens.ui;
        let active = match session.target {
            StandaloneActiveSessionTarget::Terminal(session_id) => {
                self.active_terminal_session_id(cx) == Some(session_id)
            }
            StandaloneActiveSessionTarget::RemoteDesktop(tab_id) => {
                self.active_tab_id(cx) == Some(tab_id)
            }
        };
        // The running row copies the SSH connected-row chrome exactly: same
        // Server icon, same selected background/border, same weight, no close
        // affordance. Closing stays in the right-click menu.
        let selected_bg = rgba((theme.accent << 8) | 0x1a);
        let selected_border = rgba((theme.accent << 8) | 0x4d);
        let background = if active { selected_bg } else { rgba(theme.bg << 8) };
        let border = if active { selected_border } else { rgba(theme.bg << 8) };
        let muted_text = rgb(theme.text_muted);
        let host = row.host;
        let title_str = row.title.clone();
        let row_saved_profile_id = row.saved_connection_id.clone();
        let icon = row_saved_profile_id
            .as_deref()
            .and_then(|profile_id| {
                profile_kind.map(|profile_kind| self.saved_profile_icon(profile_id, profile_kind))
            })
            .unwrap_or_else(|| standalone_session_icon(session.kind));

        div()
            .id(format!("active-session-standalone-{}", row.node_id.0))
            .h(px(SESSION_TREE_NODE_HEIGHT))
            .w_full()
            .px_2()
            .flex()
            .flex_row()
            .items_center()
            .rounded(px(self.tokens.radii.md))
            .border_b_1()
            .border_color(rgba((theme.border << 8) | SESSION_ROW_SEPARATOR_ALPHA))
            .border_1()
            .border_color(border)
            .bg(background)
            .hover(move |surface| surface.bg(rgb(theme.bg_hover)))
            .child(
                div()
                    .mr(px(8.0))
                    .flex_none()
                    .child(Self::render_lucide_icon(
                        icon,
                        SESSION_TREE_ICON_SIZE,
                        rgb(theme.text_muted),
                    )),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .truncate()
                    .text_size(px(SESSION_TREE_TEXT_SIZE))
                    .font_weight(if active {
                        gpui::FontWeight::MEDIUM
                    } else {
                        gpui::FontWeight::NORMAL
                    })
                    .text_color(rgb(theme.text))
                    .child(row.title),
            )
            .when(!host.is_empty(), |r| {
                r.child(
                    div()
                        .ml_2()
                        .max_w(px(240.0))
                        .truncate()
                        .text_size(px(SESSION_TREE_META_TEXT_SIZE))
                        .text_color(muted_text)
                        .child(host),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if standalone_session_click_should_focus(event.click_count) {
                        match session.target {
                            StandaloneActiveSessionTarget::Terminal(session_id) => {
                                this.focus_terminal_session(session_id, window, cx);
                            }
                            StandaloneActiveSessionTarget::RemoteDesktop(tab_id) => {
                                this.set_active_tab(tab_id, window, cx);
                            }
                        }
                    }
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                    this.open_active_session_context_menu(
                        row_saved_profile_id.clone(),
                        None,
                        title_str.clone(),
                        None,
                        profile_kind,
                        event.position.x.into(),
                        event.position.y.into(),
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    // =========================================================================
    // Context Menu and Dialog Actions
    // =========================================================================

    pub(in crate::workspace) fn open_active_session_context_menu(
        &mut self,
        saved_connection_id: Option<String>,
        node_id: Option<NodeId>,
        title: String,
        group: Option<String>,
        profile_kind: Option<PendingSessionProfileKind>,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        // Only one sidebar context menu can be visible at a time.
        self.active_session_folder_context_menu = None;
        self.active_session_context_menu = Some(ActiveSessionContextMenu {
            saved_connection_id,
            node_id,
            title,
            group,
            profile_kind,
            x,
            y,
        });
        cx.notify();
    }

    pub(in crate::workspace) fn close_active_session_context_menu(&mut self) -> bool {
        self.active_session_context_menu.take().is_some()
    }

    pub(in crate::workspace) fn render_active_session_context_menu(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.active_session_context_menu.as_ref()?;
        let viewport = window.viewport_size();
        const MENU_WIDTH: f32 = 180.0;
        const MENU_HEIGHT: f32 = 150.0;
        const MENU_MARGIN: f32 = 8.0;

        let placement = browser_behavior::clamp_context_menu_position(
            menu.x,
            menu.y,
            f32::from(viewport.width),
            f32::from(viewport.height),
            MENU_WIDTH,
            MENU_HEIGHT,
            MENU_MARGIN,
        );

        let saved_connection_id = menu.saved_connection_id.clone();
        let node_id = menu.node_id.clone();
        let title = menu.title.clone();
        let group = menu.group.clone();
        let profile_kind = menu.profile_kind.clone();

        let menu_body = context_menu_event_boundary(
            context_menu_content(&self.tokens)
                .w(px(MENU_WIDTH))
                .child(
                    context_menu_item(
                        &self.tokens,
                        self.i18n.t("sessions.context.edit"),
                        ContextMenuItemKind::Plain,
                        false,
                        false,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener({
                            let saved_connection_id = saved_connection_id.clone();
                            let node_id = node_id.clone();
                            let profile_kind = profile_kind.clone();
                            move |this, _event, window, cx| {
                                this.close_active_session_context_menu();
                                if let Some(id) = saved_connection_id.as_deref() {
                                    match profile_kind {
                                        Some(PendingSessionProfileKind::Telnet) => {
                                            this.open_telnet_profile_editor(id, window, cx);
                                        }
                                        Some(PendingSessionProfileKind::Serial) => {
                                            this.open_serial_profile_editor(id, window, cx);
                                        }
                                        Some(PendingSessionProfileKind::Rdp) | Some(PendingSessionProfileKind::Vnc) => {
                                            this.open_remote_desktop_profile_editor(id, window, cx);
                                        }
                                        _ => {
                                            this.open_saved_connection_editor(id, None, window, cx);
                                        }
                                    }
                                } else if let Some(node_id) = node_id.clone() {
                                    this.open_runtime_node_reconnect_editor(node_id, window, cx);
                                }
                                cx.stop_propagation();
                            }
                        }),
                    ),
                )
                .child(
                    context_menu_item(
                        &self.tokens,
                        self.i18n.t("sessions.context.duplicate"),
                        ContextMenuItemKind::Plain,
                        false,
                        saved_connection_id.is_none() || matches!(
                            profile_kind,
                            Some(PendingSessionProfileKind::Telnet)
                                | Some(PendingSessionProfileKind::Serial)
                                | Some(PendingSessionProfileKind::Rdp)
                                | Some(PendingSessionProfileKind::Vnc)
                        ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener({
                            let saved_connection_id = saved_connection_id.clone();
                            move |this, _event, _window, cx| {
                                this.close_active_session_context_menu();
                                if let Some(id) = saved_connection_id.as_deref() {
                                    if let Ok(Some(dup)) = this.connection_store.duplicate(id) {
                                        let msg = this
                                            .i18n
                                            .t("sessions.dialog.duplicate_success")
                                            .replace("{{name}}", &dup.name);
                                        this.push_command_palette_toast(
                                            msg,
                                            None,
                                            TerminalNoticeVariant::Success,
                                            cx,
                                        );
                                        this.active_session_sidebar_rows_cache.borrow_mut().take();
                                        cx.notify();
                                    }
                                }
                                cx.stop_propagation();
                            }
                        }),
                    ),
                )
                .child(
                    context_menu_item(
                        &self.tokens,
                        self.i18n.t("sessions.context.move_to_folder"),
                        ContextMenuItemKind::Plain,
                        false,
                        saved_connection_id.is_none(),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener({
                            let saved_connection_id = saved_connection_id.clone();
                            let title = title.clone();
                            let group = group.clone();
                            let profile_kind = profile_kind.clone();
                            move |this, _event, _window, cx| {
                                this.close_active_session_context_menu();
                                if let Some(id) = saved_connection_id.clone() {
                                    this.open_move_session_folder_dialog(id, title.clone(), group.clone(), profile_kind.clone(), cx);
                                }
                                cx.stop_propagation();
                            }
                        }),
                    ),
                )
                .child(context_menu_separator(&self.tokens))
                .child(
                    context_menu_item(
                        &self.tokens,
                        self.i18n.t("sessions.context.delete"),
                        ContextMenuItemKind::Plain,
                        false,
                        false,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener({
                            let saved_connection_id = saved_connection_id.clone();
                            let node_id = node_id.clone();
                            let title = title.clone();
                            let profile_kind = profile_kind.clone();
                            move |this, _event, window, cx| {
                                this.close_active_session_context_menu();
                                if let Some(id) = saved_connection_id.as_deref() {
                                    match profile_kind {
                                        Some(PendingSessionProfileKind::Telnet) => {
                                            let _ = this.connection_store.delete_telnet_profile(id);
                                        }
                                        Some(PendingSessionProfileKind::Serial) => {
                                            let _ = this.connection_store.delete_serial_profile(id);
                                        }
                                        Some(PendingSessionProfileKind::Rdp) | Some(PendingSessionProfileKind::Vnc) => {
                                            let _ = this.connection_store.delete_remote_desktop_profile(id);
                                        }
                                        _ => {
                                            let _ = this.connection_store.delete(id);
                                            if let Some(node_id) = this.saved_ssh_nodes.remove(id) {
                                                this.remove_inactive_session_tree_node(&node_id, window, cx);
                                            }
                                        }
                                    }
                                    this.cloud_sync_broadcast_snapshot(cx);
                                    let msg = this
                                        .i18n
                                        .t("sessions.dialog.delete_success")
                                        .replace("{{name}}", &title);
                                    this.push_command_palette_toast(
                                        msg,
                                        None,
                                        TerminalNoticeVariant::Success,
                                        cx,
                                    );
                                    this.active_session_sidebar_rows_cache.borrow_mut().take();
                                    cx.notify();
                                } else if let Some(ref node_id) = node_id {
                                    this.remove_inactive_session_tree_node(node_id, window, cx);
                                    this.active_session_sidebar_rows_cache.borrow_mut().take();
                                    cx.notify();
                                }
                                cx.stop_propagation();
                            }
                        }),
                    ),
                ),
        );

        let menu_body = overlay_content_boundary(menu_body);

        Some(
            self.workspace_context_menu_backdrop(
                div()
                    .absolute()
                    .top(px(placement.y))
                    .left(px(placement.x))
                    .child(menu_body),
                cx,
            )
            .into_any_element(),
        )
    }

    pub(in crate::workspace) fn open_active_session_folder_context_menu(
        &mut self,
        group: String,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        // Opening a folder menu replaces any open session menu so the two
        // sidebar overlays can never stack on top of each other.
        self.active_session_context_menu = None;
        self.active_session_folder_context_menu = Some(ActiveSessionFolderContextMenu {
            group,
            x,
            y,
        });
        cx.notify();
    }

    pub(in crate::workspace) fn close_active_session_folder_context_menu(&mut self) -> bool {
        self.active_session_folder_context_menu.take().is_some()
    }

    pub(in crate::workspace) fn render_active_session_folder_context_menu(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.active_session_folder_context_menu.as_ref()?;
        let viewport = window.viewport_size();
        const MENU_WIDTH: f32 = 180.0;
        const MENU_HEIGHT: f32 = 88.0;
        const MENU_MARGIN: f32 = 8.0;

        let placement = browser_behavior::clamp_context_menu_position(
            menu.x,
            menu.y,
            f32::from(viewport.width),
            f32::from(viewport.height),
            MENU_WIDTH,
            MENU_HEIGHT,
            MENU_MARGIN,
        );

        let group = menu.group.clone();

        let menu_body = context_menu_event_boundary(
            context_menu_content(&self.tokens)
                .w(px(MENU_WIDTH))
                .child(
                    context_menu_item(
                        &self.tokens,
                        self.i18n.t("sessions.context.rename_folder"),
                        ContextMenuItemKind::Plain,
                        false,
                        false,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener({
                            let group = group.clone();
                            move |this, _event, window, cx| {
                                this.close_active_session_folder_context_menu();
                                this.open_rename_session_folder_dialog(group.clone(), window, cx);
                                cx.stop_propagation();
                            }
                        }),
                    ),
                )
                .child(context_menu_separator(&self.tokens))
                .child(
                    context_menu_item(
                        &self.tokens,
                        self.i18n.t("sessions.context.delete_folder"),
                        ContextMenuItemKind::Plain,
                        false,
                        false,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener({
                            let group = group.clone();
                            move |this, _event, _window, cx| {
                                this.close_active_session_folder_context_menu();
                                this.open_delete_session_folder_dialog(group.clone(), cx);
                                cx.stop_propagation();
                            }
                        }),
                    ),
                ),
        );

        Some(
            self.workspace_context_menu_backdrop(
                div()
                    .absolute()
                    .top(px(placement.y))
                    .left(px(placement.x))
                    .child(menu_body),
                cx,
            )
            .into_any_element(),
        )
    }

    pub(in crate::workspace) fn open_delete_session_folder_dialog(
        &mut self,
        group: String,
        cx: &mut Context<Self>,
    ) {
        self.session_folder_delete_pending = Some(group);
        cx.notify();
    }

    pub(in crate::workspace) fn close_delete_session_folder_dialog(&mut self) -> bool {
        self.session_folder_delete_pending.take().is_some()
    }

    /// Confirms the delete-folder dialog through the single shared path used
    /// by both the destructive button and the window-level Enter routing.
    pub(in crate::workspace) fn confirm_delete_session_folder_dialog(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(group) = self.session_folder_delete_pending.clone() else {
            return;
        };
        let _ = self.connection_store.delete_group(&group);
        self.cloud_sync_broadcast_snapshot(cx);
        let msg = self
            .i18n
            .t("sessions.dialog.folder_deleted")
            .replace("{{group}}", &group);
        self.push_command_palette_toast(msg, None, TerminalNoticeVariant::Success, cx);
        self.close_delete_session_folder_dialog();
        self.active_session_sidebar_rows_cache.borrow_mut().take();
        self.expanded_ssh_nodes
            .remove(&NodeId::new(format!("folder-{group}")));
        cx.notify();
    }

    pub(in crate::workspace) fn render_delete_session_folder_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let theme = self.tokens.ui;
        let group = self.session_folder_delete_pending.as_ref()?.clone();

        let modal = div()
            .w(px(380.0))
            .p_4()
            .rounded(px(self.tokens.radii.lg))
            .bg(rgb(theme.bg_card))
            .border_1()
            .border_color(rgb(theme.border))
            .shadow_lg()
            .flex()
            .flex_col()
            .gap(px(14.0))
            // Pointer-downs inside the dialog must not reach the dismissible
            // backdrop, or clicking any content area closes the dialog.
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_base))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.text))
                    .child(self.i18n.t("sessions.dialog.delete_folder_title")),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(theme.text_muted))
                    .child(
                        self.i18n
                            .t("sessions.dialog.delete_folder_confirm")
                            .replace("{{name}}", &group),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        oxideterm_gpui_ui::button::button(
                            &self.tokens,
                            self.i18n.t("common.actions.cancel"),
                            ButtonTone::Secondary,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.close_delete_session_folder_dialog();
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        oxideterm_gpui_ui::button::button_with(
                            &self.tokens,
                            self.i18n.t("common.actions.confirm"),
                            oxideterm_gpui_ui::button::ButtonOptions {
                                variant: oxideterm_gpui_ui::button::ButtonVariant::Destructive,
                                size: oxideterm_gpui_ui::button::ButtonSize::Default,
                                radius: oxideterm_gpui_ui::button::ButtonRadius::Md,
                                disabled: false,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.confirm_delete_session_folder_dialog(cx);
                            }),
                        ),
                    ),
            );

        Some(
            dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_delete_session_folder_dialog();
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(modal),
                )
                .into_any_element(),
        )
    }

    // =========================================================================
    // Folder Dialogs
    // =========================================================================

    pub(in crate::workspace) fn open_new_session_folder_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.new_session_folder_dialog = Some(NewSessionFolderDialogState::default());
        self.ime_marked_text = None;
        self.set_ime_selection_from_anchor(WorkspaceImeTarget::NewSessionFolder, 0, 0);
        window.focus(&self.focus_handle, cx);
        self.show_active_input_caret(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn open_rename_session_folder_dialog(
        &mut self,
        group: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Prefill the current group name and leave the caret at its end so a
        // plain confirm renames without retyping the whole path.
        let caret = group.encode_utf16().count();
        let original = group.clone();
        self.new_session_folder_dialog = Some(NewSessionFolderDialogState {
            folder_name: group,
            rename_group: Some(original),
        });
        self.ime_marked_text = None;
        self.set_ime_selection_from_anchor(WorkspaceImeTarget::NewSessionFolder, caret, caret);
        window.focus(&self.focus_handle, cx);
        self.show_active_input_caret(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn close_new_session_folder_dialog(&mut self) -> bool {
        self.new_session_folder_dialog.take().is_some()
    }

    /// Confirms the new/rename folder dialog through the single shared path
    /// used by both the mouse confirm button and the window-level Enter
    /// routing, so keyboard confirm performs the identical rename/create
    /// branch as the mouse.
    pub(in crate::workspace) fn confirm_new_session_folder_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.new_session_folder_dialog.as_ref() else {
            return;
        };
        let name = dialog.folder_name.trim().to_string();
        let rename_group = dialog.rename_group.clone();
        if !name.is_empty() {
            if let Some(original) = rename_group.as_ref()
                && name != *original
            {
                let _ = self.connection_store.rename_group(original, name.clone());
                // The folder node id embeds the group name, so preserve expand
                // state across the rename instead of collapsing.
                let old_id = NodeId::new(format!("folder-{original}"));
                let new_id = NodeId::new(format!("folder-{name}"));
                if self.expanded_ssh_nodes.remove(&old_id) {
                    self.expanded_ssh_nodes.insert(new_id);
                }
                let msg = self
                    .i18n
                    .t("sessions.dialog.folder_renamed")
                    .replace("{{group}}", &name);
                self.push_command_palette_toast(msg, None, TerminalNoticeVariant::Success, cx);
            } else {
                let _ = self.connection_store.create_group(name.clone());
                // New folders open expanded so the created folder is visible
                // and its children land under it immediately.
                let folder_id = NodeId::new(format!("folder-{name}"));
                self.expanded_ssh_nodes.insert(folder_id);
                let msg = self
                    .i18n
                    .t("sessions.dialog.folder_created")
                    .replace("{{group}}", &name);
                self.push_command_palette_toast(msg, None, TerminalNoticeVariant::Success, cx);
            }
            self.active_session_sidebar_rows_cache.borrow_mut().take();
        }
        self.close_new_session_folder_dialog();
        cx.notify();
    }

    pub(in crate::workspace) fn open_move_session_folder_dialog(
        &mut self,
        connection_id: String,
        connection_title: String,
        current_group: Option<String>,
        profile_kind: Option<PendingSessionProfileKind>,
        cx: &mut Context<Self>,
    ) {
        self.move_session_folder_dialog = Some(MoveSessionFolderDialogState {
            connection_id,
            connection_title,
            current_group: current_group.clone(),
            selected_group: current_group,
            custom_folder_name: String::new(),
            is_custom: false,
            profile_kind,
        });
        cx.notify();
    }

    pub(in crate::workspace) fn close_move_session_folder_dialog(&mut self) -> bool {
        self.move_session_folder_dialog.take().is_some()
    }

    /// Option order mirrors the rendered list: the no-folder target first,
    /// then existing groups. Keyboard navigation and rendering share it.
    fn move_session_folder_options(&self) -> Vec<Option<String>> {
        let mut options = vec![None];
        options.extend(
            self.connection_store
                .groups()
                .iter()
                .map(|group| Some(group.clone())),
        );
        options
    }

    /// Moves the keyboard selection across the folder options with wraparound.
    pub(in crate::workspace) fn move_session_folder_selection(
        &mut self,
        delta: isize,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self.move_session_folder_dialog.as_ref() else {
            return;
        };
        let options = self.move_session_folder_options();
        if options.is_empty() {
            return;
        }
        let current = dialog.selected_group.clone();
        let index = options
            .iter()
            .position(|option| *option == current)
            .unwrap_or(0);
        let next = (index as isize + delta).rem_euclid(options.len() as isize) as usize;
        if let Some(dialog) = self.move_session_folder_dialog.as_mut() {
            dialog.selected_group = options[next].clone();
        }
        cx.notify();
    }

    /// Moves the pending connection into `target_group` and closes the
    /// dialog. Row clicks and the keyboard Enter routing share this path so
    /// both apply the identical move, toast, and cache invalidation.
    pub(in crate::workspace) fn confirm_move_session_folder_dialog(
        &mut self,
        target_group: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self.move_session_folder_dialog.as_ref() else {
            return;
        };
        let connection_id = dialog.connection_id.clone();
        match dialog.profile_kind {
            Some(PendingSessionProfileKind::Telnet) => {
                let _ = self.connection_store.move_session_assets_to_group(&[], &[], &[connection_id], &[], &[], target_group.as_deref());
            }
            Some(PendingSessionProfileKind::Serial) => {
                let _ = self.connection_store.move_session_assets_to_group(&[], &[connection_id], &[], &[], &[], target_group.as_deref());
            }
            Some(PendingSessionProfileKind::Rdp) | Some(PendingSessionProfileKind::Vnc) => {
                let _ = self.connection_store.move_session_assets_to_group(&[], &[], &[], &[], &[connection_id], target_group.as_deref());
            }
            _ => {
                let _ = self.connection_store.move_to_group(&[connection_id], target_group.as_deref());
            }
        }
        // The no-folder target reuses the localized option label instead of a
        // hardcoded name in the confirmation toast.
        let group_label = target_group.unwrap_or_else(|| self.i18n.t("sessions.dialog.no_folder"));
        let msg = self
            .i18n
            .t("sessions.dialog.move_success")
            .replace("{{group}}", &group_label);
        self.push_command_palette_toast(msg, None, TerminalNoticeVariant::Success, cx);
        self.active_session_sidebar_rows_cache.borrow_mut().take();
        self.close_move_session_folder_dialog();
        cx.notify();
    }

    pub(in crate::workspace) fn render_new_session_folder_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.new_session_folder_dialog.as_ref()?;
        let theme = self.tokens.ui;
        let rename_group = dialog.rename_group.clone();
        let target = WorkspaceImeTarget::NewSessionFolder;

        let input = text_input(
            &self.tokens,
            TextInputView {
                value: dialog.folder_name.as_str(),
                placeholder: self.i18n.t("sessions.dialog.new_folder_placeholder"),
                focused: true,
                caret_visible: self.input_caret.visible(),
                secret: false,
                selected_all: false,
                selected_range: self.ime_selected_range_for_target(target, cx),
                marked_text: self.marked_text_for_target(target, cx),
            },
        )
        .h(px(34.0))
        .cursor(CursorStyle::IBeam);
        let workspace = cx.entity();
        let input = text_input_anchor_probe(
            target.anchor_id(),
            input
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        this.ime_marked_text = None;
                        this.show_active_input_caret(cx);
                        window.focus(&this.focus_handle, cx);
                        this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                })),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        );

        let modal = div()
            .w(px(380.0))
            .p_4()
            .rounded(px(self.tokens.radii.lg))
            .bg(rgb(theme.bg_card))
            .border_1()
            .border_color(rgb(theme.border))
            .shadow_lg()
            .flex()
            .flex_col()
            .gap(px(14.0))
            // Pointer-downs inside the dialog must not reach the dismissible
            // backdrop, or clicking any content area closes the dialog.
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_base))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.text))
                    .child(if rename_group.is_some() {
                        self.i18n.t("sessions.dialog.rename_folder_title")
                    } else {
                        self.i18n.t("sessions.dialog.new_folder_title")
                    }),
            )
            .child(
                div()
                    .w_full()
                    .child(input),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        oxideterm_gpui_ui::button::button(
                            &self.tokens,
                            self.i18n.t("common.actions.cancel"),
                            ButtonTone::Secondary,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.close_new_session_folder_dialog();
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        oxideterm_gpui_ui::button::button(
                            &self.tokens,
                            self.i18n.t("common.actions.confirm"),
                            ButtonTone::Primary,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.confirm_new_session_folder_dialog(cx);
                            }),
                        ),
                    ),
            );

        Some(
            dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_new_session_folder_dialog();
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(modal),
                )
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn render_move_session_folder_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.move_session_folder_dialog.as_ref()?;
        let theme = self.tokens.ui;
        let connection_title = dialog.connection_title.clone();
        let groups = self.connection_store.groups().to_vec();

        let mut group_options = Vec::new();
        // Option 1: Root / No folder
        group_options.push((None, self.i18n.t("sessions.dialog.no_folder")));
        // Existing groups
        for g in groups {
            group_options.push((Some(g.clone()), g));
        }

        let modal = div()
            .w(px(380.0))
            .max_h(px(420.0))
            .p_4()
            .rounded(px(self.tokens.radii.lg))
            .bg(rgb(theme.bg_card))
            .border_1()
            .border_color(rgb(theme.border))
            .shadow_lg()
            .flex()
            .flex_col()
            .gap(px(12.0))
            // Pointer-downs inside the dialog must not reach the dismissible
            // backdrop, or clicking any content area closes the dialog.
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_base))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.text))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sessions.dialog.move_folder_title"),
                        connection_title
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .children(
                        group_options.into_iter().map(|(target_group, label)| {
                            // The highlight follows the dialog selection so
                            // keyboard navigation stays visible; it starts on
                            // the current group because selection is seeded
                            // from it when the dialog opens.
                            let is_selected = dialog.selected_group == target_group;
                            let target_grp = target_group.clone();
                            let display_label = label.clone();
                            div()
                                .px_3()
                                .py_2()
                                .rounded(px(self.tokens.radii.md))
                                .cursor_pointer()
                                .bg(if is_selected {
                                    rgba((theme.accent << 8) | 0x26)
                                } else {
                                    rgb(theme.bg)
                                })
                                .hover(move |row| row.bg(rgb(theme.bg_hover)))
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_size(px(self.tokens.metrics.ui_text_sm))
                                        .text_color(if is_selected {
                                            rgb(theme.accent)
                                        } else {
                                            rgb(theme.text)
                                        })
                                        .child(display_label.clone()),
                                )
                                .when(is_selected, |row| {
                                    row.child(Self::render_lucide_icon(
                                        LucideIcon::Check,
                                        14.0,
                                        rgb(theme.accent),
                                    ))
                                })
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _event, _window, cx| {
                                        this.confirm_move_session_folder_dialog(
                                            target_grp.clone(),
                                            cx,
                                        );
                                    }),
                                )
                        }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .child(
                        oxideterm_gpui_ui::button::button(
                            &self.tokens,
                            self.i18n.t("common.actions.cancel"),
                            ButtonTone::Secondary,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.close_move_session_folder_dialog();
                                cx.notify();
                            }),
                        ),
                    ),
            );

        Some(
            dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_move_session_folder_dialog();
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(modal),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod runtime_row_claim_tests {
    use chrono::Utc;
    use oxideterm_connections::{
        ConnectionOptions, SavedAuth, SavedConnection, SavedUpstreamProxyPolicy,
    };

    use super::{
        RuntimeRowClaimIdentity, leftover_runtime_row_is_duplicate, runtime_row_claim_index,
    };

    fn saved_connection(id: &str) -> SavedConnection {
        let now = Utc::now();
        SavedConnection {
            id: id.to_string(),
            version: 1,
            name: format!("{id} session"),
            group: None,
            notes: None,
            host: "192.0.2.10".to_string(),
            port: 22,
            username: "root".to_string(),
            auth: SavedAuth::Agent,
            proxy_chain: Vec::new(),
            upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
            proxy_command: None,
            options: ConnectionOptions::default(),
            created_at: now,
            last_used_at: None,
            updated_at: Some(now),
            color: None,
            icon_background_color: None,
            icon: None,
            tags: Vec::new(),
            post_connect_command: None,
        }
    }

    #[test]
    fn identical_endpoint_does_not_claim_runtime_row_owned_by_another_saved_id() {
        let runtime_rows = [RuntimeRowClaimIdentity {
            saved_connection_id: Some("original"),
            host: "192.0.2.10",
            username: "root",
            port: 22,
        }];

        assert_eq!(
            runtime_row_claim_index(&runtime_rows, &[false], &saved_connection("edited")),
            None
        );
    }

    #[test]
    fn identical_endpoint_can_claim_ad_hoc_runtime_row_without_saved_owner() {
        let runtime_rows = [RuntimeRowClaimIdentity {
            saved_connection_id: None,
            host: "192.0.2.10",
            username: "root",
            port: 22,
        }];

        assert_eq!(
            runtime_row_claim_index(&runtime_rows, &[false], &saved_connection("edited")),
            Some(0)
        );
    }

    #[test]
    fn leftover_owned_runtime_row_is_not_hidden_just_because_endpoint_matches() {
        let claimed = [("192.0.2.10".to_string(), 22, "root".to_string())]
            .into_iter()
            .collect();
        assert!(!leftover_runtime_row_is_duplicate(
            0,
            Some("edited"),
            "192.0.2.10",
            22,
            "root",
            &claimed,
        ));
        assert!(leftover_runtime_row_is_duplicate(
            0,
            None,
            "192.0.2.10",
            22,
            "root",
            &claimed,
        ));
    }
}
