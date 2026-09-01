// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Decoding helpers for custom themes persisted by older releases.
//!
//! The custom theme editor was removed and the built-in theme list is reduced
//! to the default dark theme. Settings files may still reference `custom:*`
//! themes, so this module keeps the JSON decoding required to render them,
//! while identifier and display-name helpers stay available for callers that
//! classify stored theme ids.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use oxideterm_settings::PersistedSettings;
use oxideterm_theme::{AppUiColors, TerminalTheme, ThemeTokens, theme_by_id};

pub const CUSTOM_THEME_PREFIX: &str = "custom:";

pub fn is_custom_theme_id(id: &str) -> bool {
    id.starts_with(CUSTOM_THEME_PREFIX)
}

pub fn custom_theme_tokens_from_settings(settings: &PersistedSettings) -> Option<ThemeTokens> {
    let (terminal, ui) = custom_theme_terminal_and_ui(settings, &settings.terminal.theme)?;
    let mut tokens = ThemeTokens::from_builtin(theme_by_id("default"));
    tokens.terminal = terminal;
    tokens.ui = ui;
    // Custom palettes must not inherit the default theme's glass contrast
    // profile, so the palette metrics are rebuilt from the decoded colors.
    tokens.refresh_palette_metrics();
    Some(tokens)
}

pub fn custom_theme_terminal_and_ui(
    settings: &PersistedSettings,
    id: &str,
) -> Option<(TerminalTheme, AppUiColors)> {
    if !is_custom_theme_id(id) {
        return None;
    }
    let value = settings.custom_themes.get(id)?;
    let terminal_value = value.get("terminalColors")?;
    let ui_value = value.get("uiColors")?;
    Some((
        terminal_theme_from_value(terminal_value)?,
        app_ui_colors_from_value(ui_value)?,
    ))
}

pub fn terminal_theme_from_value(value: &serde_json::Value) -> Option<TerminalTheme> {
    Some(TerminalTheme {
        background: color_value(value, "background")?,
        foreground: color_value(value, "foreground")?,
        cursor: color_value(value, "cursor")?,
        selection_background: intern_static_hex(color_string_value(value, "selectionBackground")?),
        black: color_value(value, "black")?,
        red: color_value(value, "red")?,
        green: color_value(value, "green")?,
        yellow: color_value(value, "yellow")?,
        blue: color_value(value, "blue")?,
        magenta: color_value(value, "magenta")?,
        cyan: color_value(value, "cyan")?,
        white: color_value(value, "white")?,
        bright_black: color_value(value, "brightBlack")?,
        bright_red: color_value(value, "brightRed")?,
        bright_green: color_value(value, "brightGreen")?,
        bright_yellow: color_value(value, "brightYellow")?,
        bright_blue: color_value(value, "brightBlue")?,
        bright_magenta: color_value(value, "brightMagenta")?,
        bright_cyan: color_value(value, "brightCyan")?,
        bright_white: color_value(value, "brightWhite")?,
    })
}

pub fn app_ui_colors_from_value(value: &serde_json::Value) -> Option<AppUiColors> {
    Some(AppUiColors {
        bg: color_value(value, "bg")?,
        bg_panel: color_value(value, "bgPanel")?,
        bg_card: color_value(value, "bgCard")?,
        bg_hover: color_value(value, "bgHover")?,
        bg_active: color_value(value, "bgActive")?,
        bg_secondary: color_value(value, "bgSecondary")?,
        bg_elevated: color_value(value, "bgElevated")?,
        bg_sunken: color_value(value, "bgSunken")?,
        text: color_value(value, "text")?,
        text_muted: color_value(value, "textMuted")?,
        text_secondary: color_value(value, "textSecondary")?,
        text_heading: color_value(value, "textHeading")
            .or_else(|| color_value(value, "text"))
            .unwrap_or(0xdbeafe),
        border: color_value(value, "border")?,
        border_strong: color_value(value, "borderStrong")?,
        divider: color_value(value, "divider")?,
        accent: color_value(value, "accent")?,
        accent_hover: color_value(value, "accentHover")?,
        accent_text: color_value(value, "accentText")?,
        accent_secondary: color_value(value, "accentSecondary")?,
        success: color_value(value, "success")?,
        warning: color_value(value, "warning")?,
        error: color_value(value, "error")?,
        info: color_value(value, "info")?,
    })
}

pub fn parse_color_hex(value: &str) -> Option<u32> {
    let trimmed = value.trim();
    if let Some(hex) = trimmed.strip_prefix('#') {
        return match hex.len() {
            3 => {
                let mut expanded = String::with_capacity(6);
                for ch in hex.chars() {
                    expanded.push(ch);
                    expanded.push(ch);
                }
                u32::from_str_radix(&expanded, 16).ok()
            }
            6 => u32::from_str_radix(hex, 16).ok(),
            8 => u32::from_str_radix(&hex[..6], 16).ok(),
            _ => None,
        };
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("rgb(") || lower.starts_with("rgba(") {
        let start = trimmed.find('(')?;
        let end = trimmed.rfind(')')?;
        let mut parts = trimmed[start + 1..end]
            .split(',')
            .map(|part| part.trim().parse::<f32>().ok());
        let red = parts.next()??.round().clamp(0.0, 255.0) as u32;
        let green = parts.next()??.round().clamp(0.0, 255.0) as u32;
        let blue = parts.next()??.round().clamp(0.0, 255.0) as u32;
        return Some((red << 16) | (green << 8) | blue);
    }

    None
}

/// Parses the strict six-digit color form used by editable color fields.
pub fn parse_rgb24_hex(value: &str) -> Option<u32> {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
}

pub fn format_hex_color(color: u32) -> String {
    format!("#{:06x}", color & 0x00ff_ffff)
}

fn color_value(value: &serde_json::Value, key: &str) -> Option<u32> {
    parse_color_hex(value.get(key)?.as_str()?)
}

fn color_string_value(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)?
        .as_str()
        .and_then(parse_color_hex)
        .map(format_hex_color)
}

/// Interns theme color strings so custom themes can be decoded repeatedly
/// without growing process memory; the set is bounded by the distinct values
/// a settings file can hold, not by how often decoding runs.
fn intern_static_hex(value: String) -> &'static str {
    static INTERNED: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let interned = INTERNED.get_or_init(|| Mutex::new(HashSet::new()));
    let mut interned = interned.lock().expect("custom theme color intern lock");
    if let Some(existing) = interned.get(value.as_str()) {
        return existing;
    }
    let leaked: &'static str = Box::leak(value.clone().into_boxed_str());
    interned.insert(leaked);
    leaked
}

pub fn theme_display_name(id: &str) -> String {
    id.split(['-', '_'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_color_hex_accepts_hex_and_rgb_forms() {
        assert_eq!(parse_color_hex("#0f0"), Some(0x00ff00));
        assert_eq!(parse_color_hex("#112233aa"), Some(0x112233));
        assert_eq!(parse_color_hex("rgb(1, 2, 3)"), Some(0x010203));
    }

    #[test]
    fn parse_rgb24_hex_requires_exactly_six_digits() {
        assert_eq!(parse_rgb24_hex(" #112233 "), Some(0x112233));
        assert_eq!(parse_rgb24_hex("112233"), Some(0x112233));
        assert_eq!(parse_rgb24_hex("#123"), None);
        assert_eq!(parse_rgb24_hex("rgb(1, 2, 3)"), None);
    }
}
