use super::*;

impl WorkspaceApp {
    // The former IDE options tab now owns a single external editor section.
    // The row reuses SettingsInput::ExternalEditorPath so this tab and the
    // General tab edit settings.general.external_editor through the same
    // draft/apply pipeline instead of maintaining a duplicate variant.
    pub(in crate::workspace) fn settings_ide_section(
        &self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        match section_index {
            0 => self.settings_card(
                "settings_view.ide.external_editor",
                "settings_view.ide.external_editor_hint",
                vec![self.setting_row(
                    "settings_view.ide.external_editor",
                    "settings_view.ide.external_editor_hint",
                    self.settings_text_input_control(
                        SettingsInput::ExternalEditorPath,
                        settings.general.external_editor.clone(),
                        "notepad++.exe".to_string(),
                        360.0,
                        cx,
                    ),
                    cx,
                )],
            ),
            _ => div().into_any_element(),
        }
    }
}
