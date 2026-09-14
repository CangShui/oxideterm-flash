use gpui::prelude::*;

use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn terminal_trigger_quick_command_pending(&self) -> bool {
        false
    }

    pub(in crate::workspace) fn handle_terminal_trigger_matches(
        &mut self,
        _pane_id: PaneId,
        _session_id: TerminalSessionId,
        _pane_owner: gpui::WeakEntity<oxideterm_gpui_terminal::TerminalPane>,
        _matches: Vec<oxideterm_terminal_triggers::TriggerMatched>,
        _cx: &mut Context<Self>,
    ) {
    }

    pub(in crate::workspace) fn clear_terminal_trigger_session_overrides(
        &mut self,
        _session_id: TerminalSessionId,
    ) {
    }

    pub(in crate::workspace) fn register_terminal_trigger_saved_connection(
        &mut self,
        _session_id: TerminalSessionId,
        _kind: oxideterm_terminal_triggers::SavedConnectionKind,
        _connection_id: String,
        _cx: &mut Context<Self>,
    ) {
    }

    pub(in crate::workspace) fn refresh_terminal_trigger_runtime(
        &mut self,
        _cx: &mut Context<Self>,
    ) {
    }

    pub(in crate::workspace) fn shutdown_terminal_trigger_runtime(&mut self) {}

    pub(in crate::workspace) fn refresh_terminal_trigger_pane(
        &mut self,
        _pane_id: PaneId,
        _cx: &mut Context<Self>,
    ) {
    }

    pub(in crate::workspace) fn handle_terminal_trigger_quick_command_key(
        &mut self,
        _event: &gpui::KeyDownEvent,
        _cx: &mut Context<Self>,
    ) -> bool {
        false
    }

    pub(in crate::workspace) fn render_terminal_trigger_quick_command_confirm(
        &self,
        _cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        None
    }
}
