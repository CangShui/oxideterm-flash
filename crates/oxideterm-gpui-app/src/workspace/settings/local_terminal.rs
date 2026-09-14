use super::*;

#[allow(dead_code)]
pub(in crate::workspace) fn semantic_scheme_label_for_id(
    settings: &PersistedSettings,
    scheme_id: &str,
    i18n: &I18n,
) -> String {
    match scheme_id {
        "balanced" => terminal_semantic_scheme_label(TerminalSemanticScheme::Balanced, i18n),
        "conservative" => {
            terminal_semantic_scheme_label(TerminalSemanticScheme::Conservative, i18n)
        }
        custom_id => settings
            .terminal
            .custom_semantic_schemes
            .iter()
            .find(|scheme| scheme.id == custom_id)
            .map(|scheme| scheme.name.clone())
            .unwrap_or_else(|| custom_id.to_string()),
    }
}

#[allow(dead_code)]
pub(in crate::workspace) fn application_semantic_scheme_label(
    settings: &PersistedSettings,
    i18n: &I18n,
) -> String {
    settings
        .terminal
        .active_custom_semantic_scheme()
        .map(|scheme| scheme.name.clone())
        .unwrap_or_else(|| terminal_semantic_scheme_label(settings.terminal.semantic_scheme, i18n))
}
