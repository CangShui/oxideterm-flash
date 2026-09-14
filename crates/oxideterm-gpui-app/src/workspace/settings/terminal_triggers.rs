use std::path::Path;

use super::*;

pub(in crate::workspace) struct TerminalTriggersSettingsState {}

impl TerminalTriggersSettingsState {
    pub(in crate::workspace) fn load(_settings_path: &Path) -> Self {
        Self {}
    }

    pub(in crate::workspace) fn cancel_edit(&mut self) {}
}

impl WorkspaceApp {
    pub(in crate::workspace) fn terminal_trigger_settings_input_value(
        &self,
        _input: SettingsInput,
    ) -> Option<String> {
        None
    }

    pub(in crate::workspace) fn apply_terminal_trigger_settings_input(
        &mut self,
        _input: SettingsInput,
        _value: &str,
    ) -> bool {
        false
    }

    pub(in crate::workspace) fn clear_terminal_trigger_input_focus(&mut self) {}
}
