// Built-in UI overrides initially ported from OxideTerm Tauri theme CSS variables.
// Only the default palette needs explicit overrides; it is registered under the
// "neutral" key so the derivation below stays byte-compatible with the Tauri port.

pub(crate) fn apply_builtin_ui_overrides(theme_id: &str, mut ui: AppUiColors) -> AppUiColors {
    match theme_id {
        "neutral" => {
            ui.bg = 0x09090b;
            ui.bg_panel = 0x18181b;
            ui.bg_card = 0x1e1e22;
            ui.bg_hover = 0x27272a;
            ui.bg_elevated = 0x1f1f23;
            ui.bg_sunken = 0x050506;
            ui.border = 0x2e2e33;
            ui.border_strong = 0x3f3f46;
            ui.text = 0xf4f4f5;
            ui.text_muted = 0xa1a1aa;
            ui.text_heading = 0xfafafa;
            ui.accent = 0xea580c;
            ui.accent_hover = 0xc2410c;
            ui.success = 0x22c55e;
            ui.warning = 0xeab308;
            ui.error = 0xef4444;
        }
        _ => {}
    }
    ui
}
