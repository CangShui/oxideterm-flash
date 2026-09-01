use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::actions::TerminalBroadcastMenuPlacement;
use super::ime::WorkspaceImeTarget;
use super::terminal_git::{
    TerminalGitBranchError, TerminalGitPanelSection, TerminalGitPathAction,
    TerminalGitRepositoryAction, terminal_git_path_action_label_key,
    terminal_git_repository_action_label_key,
};
use super::*;
use oxideterm_environment::{
    CurrentDirectoryScope, CurrentDirectorySnapshot, CurrentDirectorySource, GitChangedPath,
    GitRepositoryStatus, ProjectSnapshot, ProjectTask, ProjectTaskGroup,
};
use oxideterm_gpui_ui::button::{ButtonRadius, IconButtonOptions};
use oxideterm_gpui_ui::context_menu::{
    ContextMenuActionableStyle, context_menu_event_boundary, context_menu_pointer_event_boundary,
};
use oxideterm_gpui_ui::text_input::{TextInputView, text_input, text_input_anchor_probe};
use oxideterm_gpui_ui::{
    ActionChipOptions, ActionChipTextTone, CommandPanelOptions, ContextChipOptions,
    EntityListRowOptions, MonospaceDatumOptions, MonospaceDatumTone, StatusPillOptions, StatusTone,
    action_chip, action_chip_foreground, command_panel, command_panel_body, context_chip,
    entity_list_row, monospace_datum, status_pill,
};
use oxideterm_terminal_recording::format_recording_elapsed;

pub(in crate::workspace) mod completion;

mod bar;
mod context;
mod git;
mod highlight;
mod sender;

const TERMINAL_BROADCAST_MENU_WIDTH: f32 = 260.0;
const TERMINAL_CWD_MENU_WIDTH: f32 = 520.0;
const TERMINAL_CWD_MENU_MAX_HEIGHT: f32 = 420.0;
const TERMINAL_CWD_MENU_MARGIN: f32 = 12.0;
const TERMINAL_GIT_BRANCH_MENU_WIDTH: f32 = 460.0;
const TERMINAL_GIT_BRANCH_MENU_BODY_HEIGHT: f32 = 360.0;
const TERMINAL_GIT_BRANCH_MENU_BODY_MAX_HEIGHT: f32 = 520.0;
const TERMINAL_GIT_BRANCH_MENU_MARGIN: f32 = 12.0;
const TERMINAL_PROJECT_MENU_WIDTH: f32 = 640.0;
const TERMINAL_PROJECT_MENU_BODY_MAX_HEIGHT: f32 = 420.0;
const TERMINAL_PROJECT_MENU_MARGIN: f32 = 12.0;
const TERMINAL_COMMAND_CONTEXT_CHIP_MAX_WIDTH: f32 = 260.0; // Keep context chips compact beside command-bar actions.
const TERMINAL_COMMAND_PROJECT_CHIP_MAX_WIDTH: f32 = 240.0; // Project labels are shorter than cwd/git labels in Tauri.
const TERMINAL_COMMAND_TOOLBAR_HEIGHT: f32 = 32.0;

fn terminal_git_section_icon(section: TerminalGitPanelSection) -> LucideIcon {
    match section {
        TerminalGitPanelSection::Branches => LucideIcon::GitFork,
        TerminalGitPanelSection::Changes => LucideIcon::Pencil,
        TerminalGitPanelSection::Resolve => LucideIcon::AlertTriangle,
        TerminalGitPanelSection::History => LucideIcon::History,
        TerminalGitPanelSection::More => LucideIcon::MoreVertical,
    }
}

fn terminal_git_action_icon(action: TerminalGitRepositoryAction) -> LucideIcon {
    match action {
        TerminalGitRepositoryAction::Fetch => LucideIcon::RefreshCw,
        TerminalGitRepositoryAction::FetchAll => LucideIcon::RefreshCw,
        TerminalGitRepositoryAction::Pull => LucideIcon::Download,
        TerminalGitRepositoryAction::Push
        | TerminalGitRepositoryAction::Publish
        | TerminalGitRepositoryAction::PushTags => LucideIcon::Upload,
        TerminalGitRepositoryAction::Status => LucideIcon::ListChecks,
        TerminalGitRepositoryAction::Diff | TerminalGitRepositoryAction::DiffStaged => {
            LucideIcon::FileText
        }
        TerminalGitRepositoryAction::Log
        | TerminalGitRepositoryAction::LogStat
        | TerminalGitRepositoryAction::Reflog => LucideIcon::History,
        TerminalGitRepositoryAction::Stash => LucideIcon::Archive,
        TerminalGitRepositoryAction::StashList => LucideIcon::ListTree,
        TerminalGitRepositoryAction::StashPop => LucideIcon::Inbox,
        TerminalGitRepositoryAction::StashShowLatest => LucideIcon::FileText,
        TerminalGitRepositoryAction::StashApplyLatest => LucideIcon::Inbox,
        TerminalGitRepositoryAction::StashDropLatest => LucideIcon::Trash2,
        TerminalGitRepositoryAction::StageAll => LucideIcon::Plus,
        TerminalGitRepositoryAction::UnstageAll => LucideIcon::RotateCcw,
        TerminalGitRepositoryAction::Commit
        | TerminalGitRepositoryAction::CommitVerbose
        | TerminalGitRepositoryAction::CommitSignoff => LucideIcon::CheckCircle,
        TerminalGitRepositoryAction::Amend | TerminalGitRepositoryAction::AmendNoEdit => {
            LucideIcon::Pencil
        }
        TerminalGitRepositoryAction::RebasePull
        | TerminalGitRepositoryAction::RebaseInteractive => LucideIcon::GitFork,
        TerminalGitRepositoryAction::BranchVerbose => LucideIcon::GitFork,
        TerminalGitRepositoryAction::RemoteList => LucideIcon::Network,
        TerminalGitRepositoryAction::TagList => LucideIcon::Hash,
        TerminalGitRepositoryAction::WorktreeList => LucideIcon::FolderOpen,
        TerminalGitRepositoryAction::ConflictFiles => LucideIcon::AlertTriangle,
        TerminalGitRepositoryAction::Continue(_) => LucideIcon::Check,
        TerminalGitRepositoryAction::Abort(_) => LucideIcon::X,
        TerminalGitRepositoryAction::Skip(_) => LucideIcon::ArrowRight,
    }
}

fn terminal_project_group_icon(group: ProjectTaskGroup) -> LucideIcon {
    match group {
        ProjectTaskGroup::Develop => LucideIcon::Rocket,
        ProjectTaskGroup::Test => LucideIcon::CheckCircle,
        ProjectTaskGroup::Build => LucideIcon::FileCode,
        ProjectTaskGroup::Run => LucideIcon::Play,
        ProjectTaskGroup::Docker => LucideIcon::HardDrive,
        ProjectTaskGroup::Custom => LucideIcon::ListChecks,
    }
}

fn terminal_project_group_label_key(group: ProjectTaskGroup) -> &'static str {
    match group {
        ProjectTaskGroup::Develop => "terminal.project.group_develop",
        ProjectTaskGroup::Test => "terminal.project.group_test",
        ProjectTaskGroup::Build => "terminal.project.group_build",
        ProjectTaskGroup::Run => "terminal.project.group_run",
        ProjectTaskGroup::Docker => "terminal.project.group_docker",
        ProjectTaskGroup::Custom => "terminal.project.group_custom",
    }
}

fn terminal_cwd_chip_label(path: &str) -> String {
    let path = path.trim();
    if path.is_empty()
        || path == "/"
        || path == "~"
        || path.ends_with(":\\")
        || path.ends_with(":/")
    {
        return path.to_string();
    }
    let separator = if path.contains('\\') { '\\' } else { '/' };
    let mut segments = path
        .split(separator)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.is_empty() {
        return path.to_string();
    }
    let tail = segments.split_off(segments.len().saturating_sub(2));
    let label = tail.join(&separator.to_string());
    if path.starts_with("~/") && tail.len() == 1 {
        format!("~/{label}")
    } else {
        label
    }
}

fn terminal_cwd_chip_tooltip(
    snapshot: Option<&CurrentDirectorySnapshot>,
    host: Option<String>,
    i18n: &I18n,
) -> String {
    let Some(snapshot) = snapshot else {
        return i18n.t("terminal.cwd.unavailable");
    };
    let scope = match snapshot.scope() {
        CurrentDirectoryScope::Local => i18n.t("terminal.cwd.scope_local"),
        CurrentDirectoryScope::SshNode(_) => i18n.t("terminal.cwd.scope_ssh"),
    };
    let source = match snapshot.source() {
        CurrentDirectorySource::ProcessFallback => i18n.t("terminal.cwd.source_process"),
        CurrentDirectorySource::SessionDefault => i18n.t("terminal.cwd.source_manual"),
        CurrentDirectorySource::UserAction => i18n.t("terminal.cwd.source_manual"),
        CurrentDirectorySource::VisibleText => i18n.t("terminal.cwd.source_manual"),
        CurrentDirectorySource::ShellIntegration => i18n.t("terminal.cwd.source_shell"),
    };
    let quality = match snapshot.source() {
        CurrentDirectorySource::ProcessFallback => i18n.t("terminal.cwd.quality_process"),
        CurrentDirectorySource::SessionDefault => i18n.t("terminal.cwd.quality_manual"),
        CurrentDirectorySource::UserAction => i18n.t("terminal.cwd.quality_manual"),
        CurrentDirectorySource::VisibleText => i18n.t("terminal.cwd.quality_manual"),
        CurrentDirectorySource::ShellIntegration => i18n.t("terminal.cwd.quality_cwd_only"),
    };
    let mut lines = vec![
        snapshot.path().to_string(),
        format!("{scope} · {source} · {quality}"),
    ];
    if let Some(host) = host.filter(|host| !host.trim().is_empty()) {
        lines.push(format!("{}: {host}", i18n.t("terminal.cwd.host")));
    }
    lines.join("\n")
}

fn terminal_broadcast_menu_left_for_trigger_right(trigger_right: f32) -> f32 {
    (trigger_right - TERMINAL_BROADCAST_MENU_WIDTH).max(12.0)
}

fn terminal_cwd_browse_element_id(path: &str) -> u64 {
    // GPUI ElementId supports numeric tuple keys here; hashing keeps path-based
    // row identity stable without forcing a String into the element id type.
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}

fn terminal_project_git_root_disagreement(project_root: &str, git_root: &str) -> Option<String> {
    let project_root = project_root.trim();
    let git_root = git_root.trim();
    if project_root.is_empty() || git_root.is_empty() {
        return None;
    }

    // Project detection is manifest-based while Git detection is repository
    // based. Compare normalized display paths so nested packages surface their
    // distinct Git root without rewriting the project root.
    (terminal_display_path_key(project_root) != terminal_display_path_key(git_root))
        .then(|| git_root.to_string())
}

fn terminal_display_path_key(path: &str) -> String {
    let mut path = path.trim().replace('\\', "/");
    while path.len() > 1 && path.ends_with('/') {
        path.pop();
    }
    path
}

#[cfg(test)]
mod terminal_project_git_root_tests {
    use super::*;

    #[test]
    fn project_git_root_disagreement_ignores_trailing_separators() {
        assert_eq!(
            terminal_project_git_root_disagreement("/repo/app/", "/repo/app").as_deref(),
            None
        );
    }

    #[test]
    fn project_git_root_disagreement_reports_distinct_git_root() {
        assert_eq!(
            terminal_project_git_root_disagreement("/repo/app", "/repo").as_deref(),
            Some("/repo")
        );
    }
}
