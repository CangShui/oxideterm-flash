// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Pure settings page identity types.
//!
//! These enums describe settings navigation, editable fields, selects, and
//! sliders without depending on GPUI. View crates can map them to anchors and
//! controls, while app code can use the same model keys for focus and drafts.

const SETTINGS_SEARCH_INPUT_ANCHOR_KEY: u64 = 34_000;
const DEFAULT_SETTINGS_TEXTAREA_LINE_HEIGHT: f32 = 20.0;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SettingsTab {
    General,
    Terminal,
    Appearance,
    Connections,
    Network,
    Sftp,
    CloudSync,
    SessionIO,
    Help,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalSettingsPage {
    Display,
    Input,
    Local,
    CommandBar,
    Awareness,
    Transfer,
    Highlight,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsSelect {
    Language,
    UpdateChannel,
    UpdateProxyMode,
    UpdateProxyProtocol,
    AppearanceTheme,
    AppearanceDensity,
    AppearanceAnimation,
    AppearanceRenderProfile,
    AppearanceFrostedGlass,
    AppearanceBackgroundFit,
    TerminalFontFamily,
    TerminalCjkFontFamily,
    TerminalEncoding,
    TerminalBackspaceSequence,
    TerminalDeleteSequence,
    TerminalCursorStyle,
    TerminalTriggerMatchMode,
    TerminalTriggerAction,
    TerminalTriggerProcessMode,
    TerminalTriggerQuickCommand,
    TerminalTriggerTiming,
    TerminalTriggerScope,
    CloudSyncMode,
    LocalShell,
    LocalShellSemanticScheme(usize),
    ConnectionIdleTimeout,
    ReconnectMaxAttempts,
    ReconnectBaseDelay,
    ReconnectMaxDelay,
    NetworkApplicationProxyMode,
    NetworkProxyProtocol,
    NetworkProxyAuth,
    SftpPresentation,
    SftpProtocol,
    SftpConcurrent,
    SftpDirectoryParallelism,
    SftpConflict,
    TerminalSemanticScheme,
    SemanticSchemeRuleClass(usize),
    SemanticSchemeRuleContext(usize),
    HighlightRuleSet,
    HighlightPreset,
    HighlightRenderMode(usize),
    HighlightMatchScope(usize),
    ConnectionImportSource,
    ConnectionImportDuplicateStrategy,
    SessionExportFormat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SettingsInput {
    SettingsSearch,
    TerminalCustomFontFamily,
    TerminalFontSize,
    TerminalScrollback,
    TerminalLineHeight,
    AppearanceUiFont,
    LocalDefaultCwd,
    LocalGitBashPath,
    LocalOhMyPoshTheme,
    ConnectionDefaultUsername,
    ConnectionDefaultPort,
    ConnectionImportTargetGroup,
    ExternalEditorPath,
    CloudSyncServerUrl,
    CloudSyncRoom,
    NetworkProxyHost,
    NetworkProxyPort,
    NetworkProxyNoProxy,
    NetworkProxyUsername,
    NetworkProxyPassword,
    NetworkProxyTestHost,
    NetworkProxyTestPort,
    PublicMcpPort,
    UpdateProxyHost,
    UpdateProxyPort,
    UpdateProxyNoProxy,
    SftpSpeedLimitKbps,
    InBandTransferMaxChunkBytes,
    InBandTransferMaxFileCount,
    InBandTransferMaxTotalBytes,
    TerminalCommandBarFocusHandoff,
    TerminalCommandSpecsJson,
    TerminalTriggerName,
    TerminalTriggerDescription,
    TerminalTriggerPattern,
    TerminalTriggerActionValue,
    TerminalTriggerExecutable,
    TerminalTriggerArguments,
    TerminalTriggerWorkingDirectory,
    TerminalTriggerDelayMs,
    TerminalTriggerCooldownMs,
    SemanticSchemeName,
    SemanticSchemeRulePattern(usize),
    SemanticSchemeRuleCapture(usize),
    SemanticSchemeColor(usize),
    HighlightRuleSetName,
    HighlightLabel(usize),
    HighlightPattern(usize),
    HighlightForeground(usize),
    HighlightBackground(usize),
    ManagedKeyFilePath,
    ManagedKeyFileName,
    ManagedKeyFilePassphrase,
    ManagedKeyPasteName,
    ManagedKeyPastePrivateKey,
    ManagedKeyPastePassphrase,
    ManagedKeyRenameName,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsSlider {
    TerminalFontSize,
    AppearanceUiFontSize,
    AppearanceBorderRadius,
    OnboardingBorderRadius,
    VersionMigrationBorderRadius,
    AppearanceWindowOpacity,
    AppearanceBackgroundOpacity,
    AppearanceBackgroundBlur,
}

impl TerminalSettingsPage {
    pub fn all() -> &'static [Self] {
        &[
            Self::Display,
            Self::Input,
            Self::Local,
            Self::CommandBar,
            Self::Awareness,
            Self::Transfer,
            Self::Highlight,
        ]
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Display => "settings_view.terminal.page_display",
            Self::Input => "settings_view.terminal.page_input",
            Self::Local => "settings_view.terminal.page_local",
            Self::CommandBar => "settings_view.terminal.page_commandBar",
            Self::Awareness => "settings_view.terminal.page_awareness",
            Self::Transfer => "settings_view.terminal.page_transfer",
            Self::Highlight => "settings_view.terminal.page_highlight",
        }
    }
}

impl SettingsTab {
    pub fn all() -> &'static [Self] {
        &[
            Self::General,
            Self::Appearance,
            Self::Terminal,
            Self::Connections,
            Self::Network,
            Self::Sftp,
            Self::CloudSync,
            Self::SessionIO,
            Self::Help,
        ]
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Terminal => "terminal",
            Self::Appearance => "appearance",
            Self::Connections => "connections",
            Self::Network => "network",
            Self::Sftp => "sftp",
            Self::CloudSync => "cloud_sync",
            Self::SessionIO => "session_io",
            Self::Help => "help",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::all().iter().copied().find(|tab| tab.id() == id)
    }

    pub fn groups() -> &'static [&'static [Self]] {
        // Keep navigation groups aligned with user tasks: application preferences,
        // runtime behavior, remote access, productivity, then support.
        &[
            &[Self::General, Self::Appearance],
            &[Self::Terminal],
            &[
                Self::Connections,
                Self::Network,
                Self::Sftp,
                Self::CloudSync,
                Self::SessionIO,
            ],
            &[Self::Help],
        ]
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::General => "settings.general.title",
            Self::Terminal => "settings.terminal.title",
            Self::Appearance => "settings_view.tabs.appearance",
            Self::Connections => "settings_view.connections.keys_and_connections_title",
            Self::Network => "settings_view.tabs.network",
            Self::Sftp => "settings_view.tabs.sftp",
            Self::CloudSync => "settings_view.tabs.cloudsync",
            Self::SessionIO => "settings_view.tabs.sessionio",
            Self::Help => "settings_view.tabs.help",
        }
    }

    pub fn title_key(self) -> &'static str {
        match self {
            Self::General => "settings_view.general.title",
            Self::Terminal => "settings_view.terminal.title",
            Self::Appearance => "settings_view.appearance.title",
            Self::Connections => "settings_view.connections.keys_and_connections_title",
            Self::Network => "settings_view.network.title",
            Self::Sftp => "settings_view.sftp.title",
            Self::CloudSync => "settings_view.general.cloudsync.title",
            Self::SessionIO => "settings_view.sessionio.title",
            Self::Help => "settings_view.help.title",
        }
    }

    pub fn description_key(self) -> &'static str {
        match self {
            Self::General => "settings_view.general.description",
            Self::Terminal => "settings_view.terminal.description",
            Self::Appearance => "settings_view.appearance.description",
            Self::Connections => "settings_view.connections.keys_and_connections_description",
            Self::Network => "settings_view.network.description",
            Self::Sftp => "settings_view.sftp.description",
            Self::CloudSync => "settings_view.general.cloudsync.description",
            Self::SessionIO => "settings_view.sessionio.description",
            Self::Help => "settings_view.help.description",
        }
    }

    pub fn icon(self) -> SettingsTabIcon {
        match self {
            Self::General | Self::Appearance => SettingsTabIcon::Monitor,
            Self::Sftp => SettingsTabIcon::HardDrive,
            Self::Terminal => SettingsTabIcon::Terminal,
            Self::Connections => SettingsTabIcon::Shield,
            Self::Network => SettingsTabIcon::Network,
            Self::CloudSync => SettingsTabIcon::Sparkles,
            Self::SessionIO => SettingsTabIcon::Square,
            Self::Help => SettingsTabIcon::HelpCircle,
        }
    }
}

impl SettingsInput {
    pub fn accepts_newline(self) -> bool {
        // Keep multiline behavior beside the input identity so IME handling and
        // render controls cannot drift when new settings fields are added.
        matches!(
            self,
            Self::TerminalCommandBarFocusHandoff
                | Self::TerminalCommandSpecsJson
                | Self::TerminalTriggerArguments
                | Self::ManagedKeyPastePrivateKey
        )
    }

    pub fn textarea_line_height(self) -> f32 {
        // These values describe settings text areas in logical pixels; GPUI
        // converts them to concrete units at the view boundary.
        match self {
            Self::TerminalCommandBarFocusHandoff | Self::TerminalCommandSpecsJson => 20.0,
            Self::TerminalTriggerArguments => 20.0,
            Self::ManagedKeyPastePrivateKey => 20.0,
            _ => DEFAULT_SETTINGS_TEXTAREA_LINE_HEIGHT,
        }
    }

    pub fn anchor_key(self) -> u64 {
        match self {
            Self::SettingsSearch => SETTINGS_SEARCH_INPUT_ANCHOR_KEY,
            Self::TerminalCustomFontFamily => 19,
            Self::TerminalFontSize => 1,
            Self::TerminalScrollback => 33_000,
            Self::TerminalLineHeight => 2,
            Self::AppearanceUiFont => 5,
            Self::LocalDefaultCwd => 6,
            Self::LocalGitBashPath => 7,
            Self::LocalOhMyPoshTheme => 8,
            Self::ExternalEditorPath => 31_004,
            Self::CloudSyncServerUrl => 35_000,
            Self::CloudSyncRoom => 35_001,
            Self::ConnectionDefaultUsername => 9,
            Self::ConnectionDefaultPort => 10,
            Self::ConnectionImportTargetGroup => 20,
            Self::NetworkProxyHost => 32_000,
            Self::NetworkProxyPort => 32_001,
            Self::NetworkProxyNoProxy => 32_002,
            Self::NetworkProxyUsername => 32_003,
            Self::NetworkProxyPassword => 32_004,
            Self::NetworkProxyTestHost => 32_005,
            Self::NetworkProxyTestPort => 32_006,
            Self::PublicMcpPort => 32_007,
            Self::UpdateProxyHost => 32_100,
            Self::UpdateProxyPort => 32_101,
            Self::UpdateProxyNoProxy => 32_102,
            Self::SftpSpeedLimitKbps => 12,
            Self::InBandTransferMaxChunkBytes => 13,
            Self::InBandTransferMaxFileCount => 14,
            Self::InBandTransferMaxTotalBytes => 15,
            Self::TerminalCommandBarFocusHandoff => 16,
            Self::TerminalCommandSpecsJson => 17,
            Self::TerminalTriggerName => 33_100,
            Self::TerminalTriggerDescription => 33_101,
            Self::TerminalTriggerPattern => 33_102,
            Self::TerminalTriggerActionValue => 33_103,
            Self::TerminalTriggerExecutable => 33_104,
            Self::TerminalTriggerArguments => 33_105,
            Self::TerminalTriggerWorkingDirectory => 33_106,
            Self::TerminalTriggerDelayMs => 33_107,
            Self::TerminalTriggerCooldownMs => 33_108,
            Self::SemanticSchemeName => 10_300,
            Self::SemanticSchemeRulePattern(index) => 10_400 + index as u64,
            Self::SemanticSchemeRuleCapture(index) => 10_500 + index as u64,
            Self::SemanticSchemeColor(index) => 10_600 + index as u64,
            Self::HighlightRuleSetName => 10_700,
            Self::HighlightLabel(index) => 100 + index as u64 * 4,
            Self::HighlightPattern(index) => 101 + index as u64 * 4,
            Self::HighlightForeground(index) => 102 + index as u64 * 4,
            Self::HighlightBackground(index) => 103 + index as u64 * 4,
            Self::ManagedKeyFilePath => 30_000,
            Self::ManagedKeyFileName => 30_001,
            Self::ManagedKeyFilePassphrase => 30_002,
            Self::ManagedKeyPasteName => 30_003,
            Self::ManagedKeyPastePrivateKey => 30_004,
            Self::ManagedKeyPastePassphrase => 30_005,
            Self::ManagedKeyRenameName => 30_006,
        }
    }

    pub fn is_secret(self) -> bool {
        matches!(
            self,
            |Self::ManagedKeyFilePassphrase
                | Self::ManagedKeyPastePrivateKey
                | Self::ManagedKeyPastePassphrase
                | Self::NetworkProxyPassword
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsTabIcon {
    BookOpen,
    HardDrive,
    HelpCircle,
    Monitor,
    Network,
    Shield,
    Sparkles,
    Square,
    Terminal,
    WifiOff,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsBackgroundTabIcon {
    Activity,
    ArrowLeftRight,
    Bell,
    Cloud,
    FolderInput,
    Gauge,
    ListTree,
    Monitor,
    Network,
    Puzzle,
    Rocket,
    Settings,
    Terminal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_inputs_are_categorized_in_the_model_layer() {
        assert!(SettingsInput::NetworkProxyPassword.is_secret());
        assert!(!SettingsInput::TerminalFontSize.is_secret());
    }
}
