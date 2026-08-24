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
    Portable,
    Terminal,
    Appearance,
    Connections,
    Privilege,
    Network,
    Sftp,
    Ide,
    Keybindings,
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
pub enum SettingsKeybindingScopeFilter {
    All,
    Global,
    Terminal,
    Split,
    Palette,
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
    RemoteShellIntegrationMode,
    TerminalTriggerMatchMode,
    TerminalTriggerAction,
    TerminalTriggerProcessMode,
    TerminalTriggerQuickCommand,
    TerminalTriggerTiming,
    TerminalTriggerScope,
    IdeAgentMode,
    LocalShell,
    LocalShellSemanticScheme(usize),
    LocalPrivilegeKind,
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SettingsInput {
    SettingsSearch,
    TerminalCustomFontFamily,
    TerminalFontSize,
    TerminalScrollback,
    TerminalLineHeight,
    IdeFontSize,
    IdeLineHeight,
    AppearanceUiFont,
    LocalDefaultCwd,
    LocalGitBashPath,
    LocalOhMyPoshTheme,
    LocalPrivilegeLabel,
    LocalPrivilegeUsernameHint,
    LocalPrivilegeSecret,
    LocalPrivilegePromptPatterns,
    ConnectionDefaultUsername,
    ConnectionDefaultPort,
    ConnectionImportTargetGroup,
    ExternalEditorPath,
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
    KeybindingSearch,
    SemanticSchemeName,
    SemanticSchemeRulePattern(usize),
    SemanticSchemeRuleCapture(usize),
    SemanticSchemeColor(usize),
    HighlightRuleSetName,
    HighlightLabel(usize),
    HighlightPattern(usize),
    HighlightForeground(usize),
    HighlightBackground(usize),
    PortableCurrentPassword,
    PortableNewPassword,
    PortableConfirmPassword,
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

impl SettingsKeybindingScopeFilter {
    pub fn all() -> &'static [Self] {
        &[
            Self::All,
            Self::Global,
            Self::Terminal,
            Self::Split,
            Self::Palette,
        ]
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::All => "settings_view.keybindings.scope_all",
            Self::Global => "settings_view.keybindings.scope_global",
            Self::Terminal => "settings_view.keybindings.scope_terminal",
            Self::Split => "settings_view.keybindings.scope_split",
            Self::Palette => "settings_view.keybindings.scope_palette",
        }
    }
}

impl SettingsTab {
    pub fn all() -> &'static [Self] {
        &[
            Self::General,
            Self::Appearance,
            Self::Keybindings,
            Self::Terminal,
            Self::Portable,
            Self::Connections,
            Self::Network,
            Self::Sftp,
            Self::Privilege,
            Self::Ide,
            Self::Help,
        ]
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Portable => "portable",
            Self::Terminal => "terminal",
            Self::Appearance => "appearance",
            Self::Connections => "connections",
            Self::Privilege => "privilege",
            Self::Network => "network",
            Self::Sftp => "sftp",
            Self::Ide => "ide",
            Self::Keybindings => "keybindings",
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
            &[Self::General, Self::Appearance, Self::Keybindings],
            &[Self::Terminal, Self::Portable],
            &[
                Self::Connections,
                Self::Network,
                Self::Sftp,
                Self::Privilege,
            ],
            &[Self::Ide],
            &[Self::Help],
        ]
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::General => "settings.general.title",
            Self::Portable => "settings_view.general.portable_runtime",
            Self::Terminal => "settings.terminal.title",
            Self::Appearance => "settings_view.tabs.appearance",
            Self::Connections => "settings_view.connections.keys_and_connections_title",
            Self::Privilege => "settings_view.tabs.privilege",
            Self::Network => "settings_view.tabs.network",
            Self::Sftp => "settings_view.tabs.sftp",
            Self::Ide => "settings_view.tabs.ide",
            Self::Keybindings => "settings_view.tabs.keybindings",
            Self::Help => "settings_view.tabs.help",
        }
    }

    pub fn title_key(self) -> &'static str {
        match self {
            Self::General => "settings_view.general.title",
            Self::Portable => "settings_view.general.portable_runtime",
            Self::Terminal => "settings_view.terminal.title",
            Self::Appearance => "settings_view.appearance.title",
            Self::Connections => "settings_view.connections.keys_and_connections_title",
            Self::Privilege => "settings_view.privilege_credentials.title",
            Self::Network => "settings_view.network.title",
            Self::Sftp => "settings_view.sftp.title",
            Self::Ide => "settings_view.ide.title",
            Self::Keybindings => "settings_view.keybindings.title",
            Self::Help => "settings_view.help.title",
        }
    }

    pub fn description_key(self) -> &'static str {
        match self {
            Self::General => "settings_view.general.description",
            Self::Portable => "settings_view.general.portable_runtime_disabled_hint",
            Self::Terminal => "settings_view.terminal.description",
            Self::Appearance => "settings_view.appearance.description",
            Self::Connections => "settings_view.connections.keys_and_connections_description",
            Self::Privilege => "settings_view.privilege_credentials.description",
            Self::Network => "settings_view.network.description",
            Self::Sftp => "settings_view.sftp.description",
            Self::Ide => "settings_view.ide.description",
            Self::Keybindings => "settings_view.keybindings.description",
            Self::Help => "settings_view.help.description",
        }
    }

    pub fn icon(self) -> SettingsTabIcon {
        match self {
            Self::General | Self::Appearance => SettingsTabIcon::Monitor,
            Self::Portable | Self::Sftp => SettingsTabIcon::HardDrive,
            Self::Terminal => SettingsTabIcon::Terminal,
            Self::Connections => SettingsTabIcon::Shield,
            Self::Privilege => SettingsTabIcon::Key,
            Self::Network => SettingsTabIcon::Network,
            Self::Ide => SettingsTabIcon::Code2,
            Self::Keybindings => SettingsTabIcon::Keyboard,
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
                | Self::LocalPrivilegePromptPatterns
                | Self::ManagedKeyPastePrivateKey
        )
    }

    pub fn textarea_line_height(self) -> f32 {
        // These values describe settings text areas in logical pixels; GPUI
        // converts them to concrete units at the view boundary.
        match self {
            Self::TerminalCommandBarFocusHandoff | Self::TerminalCommandSpecsJson => 20.0,
            Self::TerminalTriggerArguments => 20.0,
            Self::LocalPrivilegePromptPatterns | Self::ManagedKeyPastePrivateKey => 20.0,
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
            Self::IdeFontSize => 3,
            Self::IdeLineHeight => 4,
            Self::AppearanceUiFont => 5,
            Self::LocalDefaultCwd => 6,
            Self::LocalGitBashPath => 7,
            Self::LocalOhMyPoshTheme => 8,
            Self::LocalPrivilegeLabel => 31_000,
            Self::LocalPrivilegeUsernameHint => 31_001,
            Self::LocalPrivilegeSecret => 31_002,
            Self::LocalPrivilegePromptPatterns => 31_003,
            Self::ExternalEditorPath => 31_004,
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
            Self::KeybindingSearch => 18,
            Self::SemanticSchemeName => 10_300,
            Self::SemanticSchemeRulePattern(index) => 10_400 + index as u64,
            Self::SemanticSchemeRuleCapture(index) => 10_500 + index as u64,
            Self::SemanticSchemeColor(index) => 10_600 + index as u64,
            Self::HighlightRuleSetName => 10_700,
            Self::HighlightLabel(index) => 100 + index as u64 * 4,
            Self::HighlightPattern(index) => 101 + index as u64 * 4,
            Self::HighlightForeground(index) => 102 + index as u64 * 4,
            Self::HighlightBackground(index) => 103 + index as u64 * 4,
            Self::PortableCurrentPassword => 28_000,
            Self::PortableNewPassword => 28_001,
            Self::PortableConfirmPassword => 28_002,
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
            |Self::PortableCurrentPassword| Self::PortableNewPassword
                | Self::PortableConfirmPassword
                | Self::LocalPrivilegeSecret
                | Self::ManagedKeyFilePassphrase
                | Self::ManagedKeyPastePrivateKey
                | Self::ManagedKeyPastePassphrase
                | Self::NetworkProxyPassword
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsTabIcon {
    BookOpen,
    Code2,
    HardDrive,
    HelpCircle,
    Key,
    Keyboard,
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
    Code2,
    Folder,
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
        assert!(SettingsInput::PortableCurrentPassword.is_secret());
        assert!(SettingsInput::PortableNewPassword.is_secret());
        assert!(SettingsInput::PortableConfirmPassword.is_secret());
        assert!(SettingsInput::LocalPrivilegeSecret.is_secret());
        assert!(!SettingsInput::TerminalFontSize.is_secret());
    }
}
