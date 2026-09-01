mod app;
mod background_cache;
mod command_facts;
mod image_budget;
mod modem_worker;
pub mod terminal_ui;
mod terminal_view;
mod trzsz_worker;

pub use app::{
    SharedTerminalSession, TerminalBroadcastInputKind, TerminalContextAction, TerminalCursorAnchor,
    TerminalCwdShellIntegrationStatus, TerminalInputBroadcaster, TerminalInputInterceptor,
    TerminalInputInterceptorResult, TerminalPane, TerminalPaneEvent, TerminalSearchStatus,
    TerminalSerialAction, TerminalSerialStatus, TerminalTelnetAction,
    TerminalWorkingDirectorySource,
};
pub use background_cache::BackgroundImageRenderCache;
pub use command_facts::{
    TerminalAiCommandRecord, TerminalAutosuggestCommandRecord, TerminalAutosuggestInputState,
    TerminalCommandFact, TerminalCommandFactStatus,
};
pub use oxideterm_terminal::TerminalOutputProcessor;
pub use oxideterm_terminal_recording::{TerminalRecordingState, TerminalRecordingStatus};
pub use oxideterm_terminal_semantic::SemanticShellDialect;
pub use terminal_ui::{
    TerminalBackgroundFit, TerminalBackgroundPreferences, TerminalCommandSelectionLabels,
    TerminalHighlightMatchScope, TerminalHighlightRenderMode, TerminalHighlightRule,
    TerminalHighlightRuleSetOverride, TerminalModemLabels, TerminalNotice, TerminalNoticeVariant,
    TerminalPasteLabels, TerminalSerialControlLabels, TerminalTrzszLabels,
    TerminalUiPreferenceOverrides, TerminalUiPreferences, TerminalUiTheme,
    resolved_terminal_semantic_scheme,
};
