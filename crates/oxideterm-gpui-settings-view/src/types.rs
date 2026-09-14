// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! GPUI adapters for pure settings model types.
//!
//! The page model enums live in `oxideterm-settings-model`; this module keeps
//! only view-layer mapping to GPUI anchors.

use oxideterm_gpui_ui::select::SelectAnchorId;
pub use oxideterm_settings_model::{
    SettingsBackgroundTabIcon, SettingsInput, SettingsSelect, SettingsSlider, SettingsTab,
    SettingsTabIcon, TerminalSettingsPage,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActiveSurface {
    Terminal,
    Settings,
}

pub fn settings_tab_from_ai_section(section: &str) -> Option<SettingsTab> {
    match section {
        "general" => Some(SettingsTab::General),
        "terminal" => Some(SettingsTab::Terminal),
        "appearance" => Some(SettingsTab::Appearance),
        "connections" | "connection_manager" => Some(SettingsTab::Connections),
        "ssh" | "ssh_keys" => Some(SettingsTab::Connections),
        "sftp" => Some(SettingsTab::Sftp),
        "help" => Some(SettingsTab::Help),
        _ => None,
    }
}

pub fn terminal_settings_page_from_ai_section(section: &str) -> Option<TerminalSettingsPage> {
    match section {
        _ => None,
    }
}

pub trait SettingsSelectAnchorExt {
    fn anchor_id(self) -> SelectAnchorId;
}

impl SettingsSelectAnchorExt for SettingsSelect {
    fn anchor_id(self) -> SelectAnchorId {
        match self {
            Self::Language => SelectAnchorId::SettingsLanguage,
            Self::UpdateChannel => SelectAnchorId::SettingsUpdateChannel,
            Self::UpdateProxyMode => SelectAnchorId::SettingsUpdateProxyMode,
            Self::UpdateProxyProtocol => SelectAnchorId::SettingsUpdateProxyProtocol,
            Self::AppearanceTheme => SelectAnchorId::SettingsAppearanceTheme,
            Self::AppearanceDensity => SelectAnchorId::SettingsAppearanceDensity,
            Self::AppearanceAnimation => SelectAnchorId::SettingsAppearanceAnimation,
            Self::AppearanceRenderProfile => SelectAnchorId::SettingsAppearanceRenderProfile,
            Self::AppearanceFrostedGlass => SelectAnchorId::SettingsAppearanceFrostedGlass,
            Self::AppearanceBackgroundFit => SelectAnchorId::SettingsAppearanceBackgroundFit,
            Self::TerminalFontFamily => SelectAnchorId::SettingsTerminalFontFamily,
            Self::TerminalCjkFontFamily => SelectAnchorId::SettingsTerminalCjkFontFamily,
            Self::TerminalEncoding => SelectAnchorId::SettingsTerminalEncoding,
            Self::TerminalBackspaceSequence => SelectAnchorId::SettingsTerminalBackspaceSequence,
            Self::TerminalDeleteSequence => SelectAnchorId::SettingsTerminalDeleteSequence,
            Self::TerminalCursorStyle => SelectAnchorId::SettingsTerminalCursorStyle,
            Self::CloudSyncMode => SelectAnchorId::SettingsCloudSyncMode,
            Self::LocalShell => SelectAnchorId::SettingsLocalShell,
            Self::LocalShellSemanticScheme(index) => {
                SelectAnchorId::SettingsLocalShellSemanticScheme(index)
            }
            Self::ConnectionIdleTimeout => SelectAnchorId::SettingsConnectionIdleTimeout,
            Self::NetworkApplicationProxyMode => {
                SelectAnchorId::SettingsNetworkApplicationProxyMode
            }
            Self::NetworkProxyProtocol => SelectAnchorId::SettingsNetworkProxyProtocol,
            Self::NetworkProxyAuth => SelectAnchorId::SettingsNetworkProxyAuth,
            Self::SftpPresentation => SelectAnchorId::SettingsSftpPresentation,
            Self::SftpProtocol => SelectAnchorId::SettingsSftpProtocol,
            Self::SftpConcurrent => SelectAnchorId::SettingsSftpConcurrent,
            Self::SftpDirectoryParallelism => SelectAnchorId::SettingsSftpDirectoryParallelism,
            Self::SftpConflict => SelectAnchorId::SettingsSftpConflict,
            Self::TerminalSemanticScheme => SelectAnchorId::SettingsTerminalSemanticScheme,
            Self::SemanticSchemeRuleClass(index) => {
                SelectAnchorId::SettingsSemanticSchemeRuleClass(index)
            }
            Self::SemanticSchemeRuleContext(index) => {
                SelectAnchorId::SettingsSemanticSchemeRuleContext(index)
            }
            Self::HighlightRuleSet => SelectAnchorId::SettingsHighlightRuleSet,
            Self::HighlightPreset => SelectAnchorId::SettingsHighlightPreset,
            Self::HighlightRenderMode(index) => SelectAnchorId::SettingsHighlightRenderMode(index),
            Self::HighlightMatchScope(index) => SelectAnchorId::SettingsHighlightMatchScope(index),
            Self::ConnectionImportSource => SelectAnchorId::SettingsConnectionImportSource,
            Self::ConnectionImportDuplicateStrategy => {
                SelectAnchorId::SettingsConnectionImportDuplicateStrategy
            }
            Self::SessionExportFormat => SelectAnchorId::SettingsSessionExportFormat,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn ai_section_aliases_map_to_settings_tabs() {
        assert_eq!(
            settings_tab_from_ai_section("ssh_keys"),
            Some(SettingsTab::Connections)
        );
        assert_eq!(settings_tab_from_ai_section("missing"), None);
    }

    #[test]
    fn every_settings_select_resolves_to_its_own_non_language_anchor() {
        // A catch-all arm placed before explicit arms silently redirected
        // every later select (import source, highlight rule set, export
        // format) to the Language anchor, so their popups opened at the
        // stale Language trigger position. Enumerate all variants so the
        // compiler keeps this table complete and distinct.
        let selects = [
            SettingsSelect::Language,
            SettingsSelect::UpdateChannel,
            SettingsSelect::UpdateProxyMode,
            SettingsSelect::UpdateProxyProtocol,
            SettingsSelect::AppearanceTheme,
            SettingsSelect::AppearanceDensity,
            SettingsSelect::AppearanceAnimation,
            SettingsSelect::AppearanceRenderProfile,
            SettingsSelect::AppearanceFrostedGlass,
            SettingsSelect::AppearanceBackgroundFit,
            SettingsSelect::TerminalFontFamily,
            SettingsSelect::TerminalCjkFontFamily,
            SettingsSelect::TerminalEncoding,
            SettingsSelect::TerminalBackspaceSequence,
            SettingsSelect::TerminalDeleteSequence,
            SettingsSelect::TerminalCursorStyle,
            SettingsSelect::CloudSyncMode,
            SettingsSelect::ConnectionIdleTimeout,
            SettingsSelect::NetworkApplicationProxyMode,
            SettingsSelect::NetworkProxyProtocol,
            SettingsSelect::NetworkProxyAuth,
            SettingsSelect::SftpPresentation,
            SettingsSelect::SftpProtocol,
            SettingsSelect::SftpConcurrent,
            SettingsSelect::SftpDirectoryParallelism,
            SettingsSelect::SftpConflict,
            SettingsSelect::TerminalSemanticScheme,
            SettingsSelect::SemanticSchemeRuleClass(0),
            SettingsSelect::SemanticSchemeRuleContext(0),
            SettingsSelect::HighlightRuleSet,
            SettingsSelect::HighlightPreset,
            SettingsSelect::HighlightRenderMode(0),
            SettingsSelect::HighlightMatchScope(0),
            SettingsSelect::ConnectionImportSource,
            SettingsSelect::ConnectionImportDuplicateStrategy,
            SettingsSelect::SessionExportFormat,
        ];
        let mut anchors = HashSet::new();
        for select in selects {
            let anchor = select.anchor_id();
            if select == SettingsSelect::Language {
                assert_eq!(anchor, SelectAnchorId::SettingsLanguage);
                continue;
            }
            assert_ne!(
                anchor,
                SelectAnchorId::SettingsLanguage,
                "{select:?} fell back to the Language anchor"
            );
            assert!(
                anchors.insert(anchor),
                "{select:?} reuses the anchor of another select"
            );
        }
    }
}
