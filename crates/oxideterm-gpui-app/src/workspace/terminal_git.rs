// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_environment::{
    GitActionPlan as TerminalGitActionPlan, GitBranchListOutcome, GitBranchReference,
    GitCommandOutput, GitOperationKind, GitProbeKey, GitProbeOutcome, GitProbeScope,
    GitRepositorySnapshot, expand_local_git_home,
    git_absolute_git_dir_args, git_action_arg_is_valid, git_branch_args, git_branch_list_args,
    git_cwd_from_directory_snapshot, git_head_args, git_operation_kind_from_git_dir,
    git_repo_root_args, git_status_args,
    git_worktree_list_args, infer_terminal_cwd_from_text, interpret_git_branch_list_outputs,
    interpret_git_command_outputs_with_status_and_operation,
    parse_shell_branch_list_output, preferred_git_cwd,
    remote_shell_branch_list_command,
};
use oxideterm_ssh::NodeId;
use tokio::process::Command;

use super::*;

#[cfg(windows)]
const TERMINAL_GIT_CREATE_NO_WINDOW: u32 = 0x08000000;
pub(super) const TERMINAL_GIT_PROBE_TTL_MS: u64 = 5_000;
pub(super) const TERMINAL_GIT_PROBE_TIMEOUT: Duration = Duration::from_secs(4);
const TERMINAL_GIT_BRANCH_LIST_TIMEOUT: Duration = Duration::from_secs(4);
pub(super) const TERMINAL_GIT_REMOTE_MAX_OUTPUT: usize = 8 * 1024;

#[derive(Debug)]
pub(in crate::workspace) enum TerminalGitDelivery {
    BranchList {
        key: GitProbeKey,
        generation: u64,
        outcome: GitBranchListOutcome,
    },
}

#[derive(Debug, Eq, PartialEq)]
pub(in crate::workspace) enum TerminalGitBranchError {
    NotRepository,
    GitUnavailable,
    CwdUnavailable,
    NodeUnavailable,
    Message(String),
}

#[derive(Default)]
pub(in crate::workspace) struct TerminalGitBranchPickerState {
    open: bool,
    active_section: TerminalGitPanelSection,
    key: Option<GitProbeKey>,
    query: String,
    commit_message: String,
    branches: Vec<GitBranchReference>,
    highlighted_branch: Option<String>,
    loading: bool,
    error: Option<TerminalGitBranchError>,
    generation: u64,
}

impl TerminalGitBranchPickerState {
    fn next_generation(&mut self) -> u64 {
        self.generation = self.generation.saturating_add(1);
        self.generation
    }

    fn close(&mut self) {
        let generation = self.generation;
        *self = Self::default();
        // Generations belong to the Entity lifetime, not one panel mount. Keep
        // them monotonic across panel mounts.
        self.generation = generation;
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::workspace) enum TerminalGitPanelSection {
    Branches,
    #[default]
    Changes,
    Resolve,
    History,
    More,
}

impl TerminalGitPanelSection {
    pub(in crate::workspace) fn label_key(self) -> &'static str {
        match self {
            Self::Branches => "terminal.git.section_branches",
            Self::Changes => "terminal.git.section_changes",
            Self::Resolve => "terminal.git.section_resolve",
            Self::History => "terminal.git.section_history",
            Self::More => "terminal.git.section_more",
        }
    }
}

pub(in crate::workspace) use oxideterm_environment::{
    GitPathAction as TerminalGitPathAction, GitRepositoryAction as TerminalGitRepositoryAction,
};

// Translation ownership stays in the app instead of leaking UI keys into the domain crate.
pub(in crate::workspace) fn terminal_git_repository_action_label_key(
    action: TerminalGitRepositoryAction,
) -> &'static str {
    match action {
        TerminalGitRepositoryAction::Fetch => "terminal.git.action_fetch",
        TerminalGitRepositoryAction::Pull => "terminal.git.action_pull",
        TerminalGitRepositoryAction::Push => "terminal.git.action_push",
        TerminalGitRepositoryAction::Publish => "terminal.git.action_publish",
        TerminalGitRepositoryAction::Status => "terminal.git.action_status",
        TerminalGitRepositoryAction::Diff => "terminal.git.action_diff",
        TerminalGitRepositoryAction::DiffStaged => "terminal.git.action_diff_staged",
        TerminalGitRepositoryAction::Log => "terminal.git.action_log",
        TerminalGitRepositoryAction::Stash => "terminal.git.action_stash",
        TerminalGitRepositoryAction::StashList => "terminal.git.action_stash_list",
        TerminalGitRepositoryAction::StashPop => "terminal.git.action_stash_pop",
        TerminalGitRepositoryAction::StageAll => "terminal.git.action_stage_all",
        TerminalGitRepositoryAction::UnstageAll => "terminal.git.action_unstage_all",
        TerminalGitRepositoryAction::Commit => "terminal.git.action_commit",
        TerminalGitRepositoryAction::CommitVerbose => "terminal.git.action_commit_verbose",
        TerminalGitRepositoryAction::CommitSignoff => "terminal.git.action_commit_signoff",
        TerminalGitRepositoryAction::Amend => "terminal.git.action_amend",
        TerminalGitRepositoryAction::AmendNoEdit => "terminal.git.action_amend_no_edit",
        TerminalGitRepositoryAction::RebasePull => "terminal.git.action_rebase_pull",
        TerminalGitRepositoryAction::RebaseInteractive => "terminal.git.action_rebase_interactive",
        TerminalGitRepositoryAction::FetchAll => "terminal.git.action_fetch_all",
        TerminalGitRepositoryAction::PushTags => "terminal.git.action_push_tags",
        TerminalGitRepositoryAction::LogStat => "terminal.git.action_log_stat",
        TerminalGitRepositoryAction::Reflog => "terminal.git.action_reflog",
        TerminalGitRepositoryAction::BranchVerbose => "terminal.git.action_branch_verbose",
        TerminalGitRepositoryAction::RemoteList => "terminal.git.action_remote_list",
        TerminalGitRepositoryAction::TagList => "terminal.git.action_tag_list",
        TerminalGitRepositoryAction::WorktreeList => "terminal.git.action_worktree_list",
        TerminalGitRepositoryAction::StashShowLatest => "terminal.git.action_stash_show_latest",
        TerminalGitRepositoryAction::StashApplyLatest => "terminal.git.action_stash_apply_latest",
        TerminalGitRepositoryAction::StashDropLatest => "terminal.git.action_stash_drop_latest",
        TerminalGitRepositoryAction::ConflictFiles => "terminal.git.action_conflict_files",
        TerminalGitRepositoryAction::Continue(_) => "terminal.git.action_continue",
        TerminalGitRepositoryAction::Abort(_) => "terminal.git.action_abort",
        TerminalGitRepositoryAction::Skip(_) => "terminal.git.action_skip",
    }
}

// Path-action labels follow the same app-owned internationalization boundary.
pub(in crate::workspace) fn terminal_git_path_action_label_key(
    action: TerminalGitPathAction,
) -> &'static str {
    match action {
        TerminalGitPathAction::Stage => "terminal.git.path_action_stage",
        TerminalGitPathAction::Unstage => "terminal.git.path_action_unstage",
        TerminalGitPathAction::Diff => "terminal.git.path_action_diff",
        TerminalGitPathAction::DiffStaged => "terminal.git.path_action_diff_staged",
        TerminalGitPathAction::Open => "terminal.git.path_action_open",
        TerminalGitPathAction::Ours => "terminal.git.path_action_ours",
        TerminalGitPathAction::Theirs => "terminal.git.path_action_theirs",
    }
}

impl WorkspaceTerminalEntity {
    pub(in crate::workspace) fn git_panel_open(&self) -> bool {
        self.git_panel.open
    }

    pub(in crate::workspace) fn git_panel_active_section(&self) -> TerminalGitPanelSection {
        self.git_panel.active_section
    }

    pub(in crate::workspace) fn set_git_panel_active_section(
        &mut self,
        section: TerminalGitPanelSection,
    ) {
        self.git_panel.active_section = section;
    }

    pub(in crate::workspace) fn git_panel_query(&self) -> &str {
        &self.git_panel.query
    }

    pub(in crate::workspace) fn git_commit_message(&self) -> &str {
        &self.git_panel.commit_message
    }

    pub(in crate::workspace) fn git_commit_message_ready(&self) -> bool {
        TerminalGitActionPlan::commit_message(&self.git_panel.commit_message).is_some()
    }

    pub(in crate::workspace) fn git_panel_loading(&self) -> bool {
        self.git_panel.loading
    }

    pub(in crate::workspace) fn git_panel_error(&self) -> Option<&TerminalGitBranchError> {
        self.git_panel.error.as_ref()
    }

    pub(in crate::workspace) fn set_git_panel_error(&mut self, error: TerminalGitBranchError) {
        if self.git_panel.open {
            self.git_panel.loading = false;
            self.git_panel.error = Some(error);
        }
    }

    pub(in crate::workspace) fn open_git_panel(
        &mut self,
        key: GitProbeKey,
        active_section: TerminalGitPanelSection,
        cx: &mut Context<Self>,
    ) {
        let generation = self.git_panel.next_generation();
        self.git_panel.open = true;
        self.git_panel.active_section = active_section;
        self.git_panel.key = Some(key.clone());
        self.git_panel.query.clear();
        self.git_panel.commit_message.clear();
        self.git_panel.branches.clear();
        self.git_panel.highlighted_branch = None;
        self.git_panel.loading = true;
        self.git_panel.error = None;
        self.spawn_git_branch_list(key, generation, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn close_git_panel(&mut self) -> bool {
        let was_open = self.git_panel.open;
        if was_open {
            self.git_panel.close();
        }
        was_open
    }

    pub(in crate::workspace) fn replace_git_panel_query(
        &mut self,
        replacement_range: Option<std::ops::Range<usize>>,
        text: &str,
    ) -> bool {
        if !self.git_panel.open {
            return false;
        }
        replace_utf16(&mut self.git_panel.query, replacement_range, text);
        self.git_panel.highlighted_branch = None;
        self.ensure_git_branch_highlight();
        true
    }

    pub(in crate::workspace) fn replace_git_commit_message(
        &mut self,
        replacement_range: Option<std::ops::Range<usize>>,
        text: &str,
    ) -> bool {
        if !self.git_panel.open {
            return false;
        }
        replace_utf16(&mut self.git_panel.commit_message, replacement_range, text);
        true
    }

    pub(in crate::workspace) fn visible_git_branches(&self) -> Vec<GitBranchReference> {
        let query = self.git_panel.query.trim().to_ascii_lowercase();
        self.git_panel
            .branches
            .iter()
            .filter(|branch| {
                query.is_empty() || branch.name().to_ascii_lowercase().contains(&query)
            })
            .cloned()
            .collect()
    }

    pub(in crate::workspace) fn git_query_checkout_candidate(&self) -> Option<String> {
        let query = self.git_panel.query.trim();
        if !git_action_arg_is_valid(query)
            || self
                .git_panel
                .branches
                .iter()
                .any(|branch| branch.name() == query)
        {
            return None;
        }
        Some(query.to_string())
    }

    pub(in crate::workspace) fn git_query_rebase_candidate(
        &self,
        current_branch: Option<&str>,
    ) -> Option<String> {
        let query = self.git_panel.query.trim();
        if !git_action_arg_is_valid(query) || current_branch == Some(query) {
            return None;
        }
        Some(query.to_string())
    }

    pub(in crate::workspace) fn git_query_create_branch_candidate(&self) -> Option<String> {
        let query = self.git_panel.query.trim();
        if !git_action_arg_is_valid(query)
            || self
                .git_panel
                .branches
                .iter()
                .any(|branch| branch.name() == query)
        {
            return None;
        }
        Some(query.to_string())
    }

    pub(in crate::workspace) fn git_query_remote_tracking_candidate(&self) -> Option<String> {
        let query = self.git_panel.query.trim();
        if !git_action_arg_is_valid(query) || !query.contains('/') {
            return None;
        }
        self.git_panel
            .branches
            .iter()
            .all(|branch| branch.name() != query)
            .then(|| query.to_string())
    }

    pub(in crate::workspace) fn git_branch_highlighted(&self, branch_name: &str) -> bool {
        self.git_panel.highlighted_branch.as_deref() == Some(branch_name)
    }

    pub(in crate::workspace) fn git_worktree_branch_count(&self) -> usize {
        self.git_panel
            .branches
            .iter()
            .filter(|branch| branch.worktree_path().is_some())
            .count()
    }

    pub(in crate::workspace) fn set_git_branch_highlight(&mut self, branch_name: &str) -> bool {
        if self.git_branch_highlighted(branch_name) {
            return false;
        }
        self.git_panel.highlighted_branch = Some(branch_name.to_string());
        true
    }

    pub(in crate::workspace) fn selected_git_branch(&self) -> Option<GitBranchReference> {
        let visible = self.visible_git_branches();
        self.git_panel
            .highlighted_branch
            .as_deref()
            .and_then(|highlighted| {
                visible
                    .iter()
                    .find(|branch| branch.name() == highlighted)
                    .cloned()
            })
            .or_else(|| visible.first().cloned())
    }

    pub(in crate::workspace) fn step_git_branch_highlight(&mut self, forward: bool) {
        let visible = self.visible_git_branches();
        if visible.is_empty() {
            self.git_panel.highlighted_branch = None;
            return;
        }
        let current = self
            .git_panel
            .highlighted_branch
            .as_deref()
            .and_then(|highlighted| {
                visible
                    .iter()
                    .position(|branch| branch.name() == highlighted)
            });
        let next = match (current, forward) {
            (Some(index), true) => (index + 1).min(visible.len() - 1),
            (Some(index), false) => index.saturating_sub(1),
            (None, true) => 0,
            (None, false) => visible.len() - 1,
        };
        self.git_panel.highlighted_branch = Some(visible[next].name().to_string());
    }

    pub(in crate::workspace) fn highlight_git_branch_edge(&mut self, last: bool) {
        let visible = self.visible_git_branches();
        self.git_panel.highlighted_branch = if last {
            visible.last()
        } else {
            visible.first()
        }
        .map(|branch| branch.name().to_string());
    }

    pub(super) fn drain_git_action_results(&mut self, cx: &mut Context<Self>) -> bool {
        let delivery_batch =
            delivery::drain_channel(&self.git_action_rx, delivery::USER_ACTION_DELIVERY_BUDGET);
        let mut changed = false;
        for delivery in delivery_batch.items {
            match delivery {
                TerminalGitDelivery::BranchList {
                    key,
                    generation,
                    outcome,
                } => {
                    changed |= self.apply_git_branch_list_result(key, generation, outcome);
                }
            }
        }
        if changed {
            cx.notify();
        }
        delivery_batch.outcome.backlog_remaining
    }

    fn spawn_git_branch_list(&mut self, key: GitProbeKey, generation: u64, cx: &mut Context<Self>) {
        match key.scope() {
            GitProbeScope::Local => self.spawn_local_git_branch_list(key, generation),
            GitProbeScope::SshNode(node_id) => {
                let node_id = NodeId::new(node_id.clone());
                self.spawn_remote_git_branch_list(key, generation, node_id, cx);
            }
        }
    }

    fn spawn_local_git_branch_list(&self, key: GitProbeKey, generation: u64) {
        let tx = self.git_action_tx.clone();
        let cwd = key.cwd().to_string();
        self.runtime.spawn(async move {
            let outcome = run_local_git_branch_list(&cwd).await;
            let _ = tx.send(TerminalGitDelivery::BranchList {
                key,
                generation,
                outcome,
            });
        });
    }

    fn spawn_remote_git_branch_list(
        &mut self,
        key: GitProbeKey,
        generation: u64,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) {
        let resolved = self.node_router.resolve_connection_now(&node_id);
        let handle = match resolved {
            Ok(resolved) => resolved.handle,
            Err(_) => {
                self.git_panel.loading = false;
                self.git_panel.error = Some(TerminalGitBranchError::NodeUnavailable);
                cx.notify();
                return;
            }
        };

        let tx = self.git_action_tx.clone();
        let command = remote_shell_branch_list_command(key.cwd());
        self.runtime.spawn(async move {
            let outcome = match handle
                .run_command_capture(
                    &command,
                    TERMINAL_GIT_BRANCH_LIST_TIMEOUT,
                    TERMINAL_GIT_REMOTE_MAX_OUTPUT,
                )
                .await
            {
                Ok(output) => parse_shell_branch_list_output(&output.stdout),
                Err(_) => GitBranchListOutcome::Error(oxideterm_environment::GitProbeError::new(
                    "ssh git branch list failed",
                )),
            };
            let _ = tx.send(TerminalGitDelivery::BranchList {
                key,
                generation,
                outcome,
            });
        });
    }

    fn apply_git_branch_list_result(
        &mut self,
        key: GitProbeKey,
        generation: u64,
        outcome: GitBranchListOutcome,
    ) -> bool {
        if !self.git_panel.open
            || self.git_panel.key.as_ref() != Some(&key)
            || self.git_panel.generation != generation
        {
            return false;
        }

        self.git_panel.loading = false;
        match outcome {
            GitBranchListOutcome::Ready(branches) => {
                self.git_panel.error = None;
                self.git_panel.highlighted_branch = branches
                    .iter()
                    .find(|branch| branch.current())
                    .or_else(|| branches.first())
                    .map(|branch| branch.name().to_string());
                self.git_panel.branches = branches;
            }
            GitBranchListOutcome::NotRepository => {
                self.git_panel.branches.clear();
                self.git_panel.highlighted_branch = None;
                self.git_panel.error = Some(TerminalGitBranchError::NotRepository);
            }
            GitBranchListOutcome::GitUnavailable => {
                self.git_panel.branches.clear();
                self.git_panel.highlighted_branch = None;
                self.git_panel.error = Some(TerminalGitBranchError::GitUnavailable);
            }
            GitBranchListOutcome::CwdUnavailable => {
                self.git_panel.branches.clear();
                self.git_panel.highlighted_branch = None;
                self.git_panel.error = Some(TerminalGitBranchError::CwdUnavailable);
            }
            GitBranchListOutcome::Error(error) => {
                self.git_panel.branches.clear();
                self.git_panel.highlighted_branch = None;
                self.git_panel.error =
                    Some(TerminalGitBranchError::Message(error.message().to_string()));
            }
        }
        true
    }

    fn ensure_git_branch_highlight(&mut self) {
        let visible = self.visible_git_branches();
        if visible
            .iter()
            .any(|branch| Some(branch.name()) == self.git_panel.highlighted_branch.as_deref())
        {
            return;
        }
        self.git_panel.highlighted_branch = visible.first().map(|branch| branch.name().to_string());
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn active_terminal_git_snapshot(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<GitRepositorySnapshot> {
        let key = self.active_terminal_git_key(cx)?;
        self.terminal.read(cx).git_snapshot(&key)
    }

    pub(in crate::workspace) fn open_terminal_git_branch_picker(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.active_terminal_git_key(cx) else {
            return;
        };
        let active_section = if self
            .active_terminal_git_snapshot(cx)
            .and_then(|snapshot| snapshot.status.operation())
            .is_some()
        {
            TerminalGitPanelSection::Resolve
        } else {
            TerminalGitPanelSection::Changes
        };

        self.dismiss_terminal_recording_menu();
        self.close_terminal_quick_commands_popover(cx);
        self.dismiss_terminal_broadcast_menu(cx);
        self.dismiss_terminal_highlight_popover();
        self.close_terminal_cwd_picker(cx);
        self.close_terminal_project_panel(cx);
        self.ime_marked_text = None;
        self.clear_ime_selection();

        self.terminal.update(cx, |terminal, cx| {
            terminal.open_git_panel(key, active_section, cx)
        });
        cx.notify();
    }

    pub(in crate::workspace) fn close_terminal_git_branch_picker(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let was_open = self
            .terminal
            .update(cx, |terminal, _cx| terminal.close_git_panel());
        if was_open {
            self.ime_marked_text = None;
            self.clear_ime_selection();
        }
        was_open
    }

    pub(in crate::workspace) fn visible_terminal_git_branches(
        &self,
        cx: &App,
    ) -> Vec<GitBranchReference> {
        self.terminal.read(cx).visible_git_branches()
    }

    pub(in crate::workspace) fn terminal_git_query_checkout_candidate(
        &self,
        cx: &App,
    ) -> Option<String> {
        self.terminal.read(cx).git_query_checkout_candidate()
    }

    pub(in crate::workspace) fn terminal_git_query_rebase_candidate(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let current_branch = self
            .active_terminal_git_snapshot(cx)
            .map(|snapshot| snapshot.branch.display_text().to_string());
        self.terminal
            .read(cx)
            .git_query_rebase_candidate(current_branch.as_deref())
    }

    pub(in crate::workspace) fn terminal_git_query_create_branch_candidate(
        &self,
        cx: &App,
    ) -> Option<String> {
        self.terminal.read(cx).git_query_create_branch_candidate()
    }

    pub(in crate::workspace) fn terminal_git_query_remote_tracking_candidate(
        &self,
        cx: &App,
    ) -> Option<String> {
        self.terminal.read(cx).git_query_remote_tracking_candidate()
    }

    pub(in crate::workspace) fn checkout_terminal_git_query(&mut self, cx: &mut Context<Self>) {
        let Some(branch_name) = self.terminal_git_query_checkout_candidate(cx) else {
            return;
        };
        let Some(plan) = TerminalGitActionPlan::checkout_name(&branch_name) else {
            return;
        };
        let failure_message =
            self.i18n_replace("terminal.git.checkout_failed", &[("branch", branch_name)]);
        self.send_terminal_git_command(plan, failure_message, cx);
    }

    pub(in crate::workspace) fn rebase_terminal_git_query(&mut self, cx: &mut Context<Self>) {
        let Some(branch_name) = self.terminal_git_query_rebase_candidate(cx) else {
            return;
        };
        let Some(plan) = TerminalGitActionPlan::rebase_onto_name(&branch_name) else {
            return;
        };
        let action_label = self.i18n.t("terminal.git.action_rebase");
        let failure_message =
            self.i18n_replace("terminal.git.command_failed", &[("action", action_label)]);
        self.send_terminal_git_command(plan, failure_message, cx);
    }

    pub(in crate::workspace) fn create_terminal_git_query_branch(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(branch_name) = self.terminal_git_query_create_branch_candidate(cx) else {
            return;
        };
        let Some(plan) = TerminalGitActionPlan::create_branch_name(&branch_name) else {
            return;
        };
        let failure_message =
            self.i18n_replace("terminal.git.checkout_failed", &[("branch", branch_name)]);
        self.send_terminal_git_command(plan, failure_message, cx);
    }

    pub(in crate::workspace) fn rename_terminal_git_query_branch(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(branch_name) = self.terminal_git_query_create_branch_candidate(cx) else {
            return;
        };
        let Some(plan) = TerminalGitActionPlan::rename_current_branch(&branch_name) else {
            return;
        };
        let action_label = self.i18n.t("terminal.git.action_branch_rename");
        let failure_message =
            self.i18n_replace("terminal.git.command_failed", &[("action", action_label)]);
        self.send_terminal_git_command(plan, failure_message, cx);
    }

    pub(in crate::workspace) fn track_terminal_git_query_remote_branch(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(branch_name) = self.terminal_git_query_remote_tracking_candidate(cx) else {
            return;
        };
        let Some(plan) = TerminalGitActionPlan::track_remote_branch(&branch_name) else {
            return;
        };
        let action_label = self.i18n.t("terminal.git.action_branch_track_remote");
        let failure_message =
            self.i18n_replace("terminal.git.command_failed", &[("action", action_label)]);
        self.send_terminal_git_command(plan, failure_message, cx);
    }

    pub(in crate::workspace) fn select_terminal_git_branch(
        &mut self,
        branch: GitBranchReference,
        cx: &mut Context<Self>,
    ) {
        let branch_name = branch.name().to_string();
        if branch_name.trim().is_empty() {
            return;
        }
        if branch.current() {
            self.close_terminal_git_branch_picker(cx);
            cx.notify();
            return;
        }

        let Some(plan) = TerminalGitActionPlan::select_branch(&branch) else {
            return;
        };
        let failure_message =
            self.i18n_replace("terminal.git.checkout_failed", &[("branch", branch_name)]);
        self.send_terminal_git_command(plan, failure_message, cx);
    }

    pub(in crate::workspace) fn run_terminal_git_repository_action(
        &mut self,
        action: TerminalGitRepositoryAction,
        cx: &mut Context<Self>,
    ) {
        let plan = TerminalGitActionPlan::repository_action(action);
        let action_label = self
            .i18n
            .t(terminal_git_repository_action_label_key(action));
        let failure_message =
            self.i18n_replace("terminal.git.command_failed", &[("action", action_label)]);
        self.send_terminal_git_command(plan, failure_message, cx);
    }

    pub(in crate::workspace) fn commit_terminal_git_message(&mut self, cx: &mut Context<Self>) {
        let message = self.terminal.read(cx).git_commit_message().to_string();
        let Some(plan) = TerminalGitActionPlan::commit_message(&message) else {
            return;
        };
        let action_label = self.i18n.t("terminal.git.action_commit");
        let failure_message =
            self.i18n_replace("terminal.git.command_failed", &[("action", action_label)]);
        self.send_terminal_git_command(plan, failure_message, cx);
    }

    pub(in crate::workspace) fn run_terminal_git_path_action(
        &mut self,
        action: TerminalGitPathAction,
        path: String,
        cx: &mut Context<Self>,
    ) {
        let Some(plan) = TerminalGitActionPlan::path_action(action, &path) else {
            return;
        };
        let action_label = self.i18n.t(terminal_git_path_action_label_key(action));
        let failure_message =
            self.i18n_replace("terminal.git.command_failed", &[("action", action_label)]);
        self.send_terminal_git_command(plan, failure_message, cx);
    }

    fn send_terminal_git_command(
        &mut self,
        plan: TerminalGitActionPlan,
        failure_message: String,
        cx: &mut Context<Self>,
    ) {
        let Some(pane_id) = self.active_pane_id(cx) else {
            self.terminal.update(cx, |terminal, _cx| {
                terminal.set_git_panel_error(TerminalGitBranchError::Message(failure_message));
            });
            cx.notify();
            return;
        };
        let Some(pane) = self.tab_host.read(cx).panes().get(&pane_id).cloned() else {
            self.terminal.update(cx, |terminal, _cx| {
                terminal.set_git_panel_error(TerminalGitBranchError::Message(failure_message));
            });
            cx.notify();
            return;
        };
        // Git actions are sent through the active terminal so the user sees
        // Git's own output, conflict prompts, and any recovery instructions.
        pane.update(cx, |pane, cx| {
            pane.send_command_line(plan.command(), cx);
            if let Some(cwd) = plan.cwd_after_command() {
                pane.set_current_working_directory_from_terminal_action(cwd.to_string(), cx);
            }
        });
        self.close_terminal_git_branch_picker(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn handle_terminal_git_branch_picker_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.terminal.read(cx).git_panel_open() {
            return false;
        }
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform || modifiers.control || modifiers.alt {
            return false;
        }

        match key {
            "escape" => {
                self.close_terminal_git_branch_picker(cx);
                cx.notify();
                true
            }
            "up" | "arrowup" => {
                if self.terminal.read(cx).git_panel_active_section()
                    != TerminalGitPanelSection::Branches
                {
                    return false;
                }
                self.terminal.update(cx, |terminal, _cx| {
                    terminal.step_git_branch_highlight(false)
                });
                cx.notify();
                true
            }
            "down" | "arrowdown" => {
                if self.terminal.read(cx).git_panel_active_section()
                    != TerminalGitPanelSection::Branches
                {
                    return false;
                }
                self.terminal
                    .update(cx, |terminal, _cx| terminal.step_git_branch_highlight(true));
                cx.notify();
                true
            }
            "home" => {
                if self.terminal.read(cx).git_panel_active_section()
                    != TerminalGitPanelSection::Branches
                {
                    return false;
                }
                self.terminal.update(cx, |terminal, _cx| {
                    terminal.highlight_git_branch_edge(false)
                });
                cx.notify();
                true
            }
            "end" => {
                if self.terminal.read(cx).git_panel_active_section()
                    != TerminalGitPanelSection::Branches
                {
                    return false;
                }
                self.terminal
                    .update(cx, |terminal, _cx| terminal.highlight_git_branch_edge(true));
                cx.notify();
                true
            }
            "enter" => {
                let active_section = self.terminal.read(cx).git_panel_active_section();
                match active_section {
                    TerminalGitPanelSection::Branches => {
                        let branch = self.terminal.read(cx).selected_git_branch();
                        if let Some(branch) = branch {
                            self.select_terminal_git_branch(branch, cx);
                        }
                    }
                    TerminalGitPanelSection::Changes => {
                        let can_commit = self.terminal.read(cx).git_commit_message_ready()
                            && self
                                .active_terminal_git_snapshot(cx)
                                .is_some_and(|snapshot| snapshot.status.staged() > 0);
                        if can_commit {
                            self.commit_terminal_git_message(cx);
                        }
                    }
                    _ => {}
                }
                // Enter belongs to the open Git panel and must never reach the
                // terminal pane behind it, even when the commit is disabled.
                true
            }
            _ => false,
        }
    }

    pub(in crate::workspace) fn active_terminal_git_key(&self, cx: &App) -> Option<GitProbeKey> {
        let command_bar_settings = &self.settings_store.settings().terminal.command_bar;
        if !command_bar_settings.enabled || !command_bar_settings.git_status {
            return None;
        }

        let tab = self.active_tab(cx)?;
        let tab_kind = tab.kind.clone();
        let pane_id = tab.active_pane_id?;
        let scope = match tab_kind {
            TabKind::SshTerminal => {
                let session_id = self.active_terminal_session_id(cx)?;
                let node_id = self
                    .workspace_runtime
                    .read(cx)
                    .ssh_terminal_node_id(session_id)?;
                GitProbeScope::ssh_node(node_id.0.clone())
            }
            _ => return None,
        };
        let cwd = self.active_terminal_git_cwd(pane_id, &scope, cx)?;

        GitProbeKey::new(scope, cwd)
    }

    fn active_terminal_git_cwd(
        &self,
        pane_id: PaneId,
        scope: &GitProbeScope,
        cx: &App,
    ) -> Option<String> {
        let snapshot_cwd = self
            .active_terminal_cwd_snapshot(cx)
            .and_then(|snapshot| git_cwd_from_directory_snapshot(scope, &snapshot));
        let tab_host = self.tab_host.read(cx);
        let pane = tab_host.panes().get(&pane_id)?;
        let pane = pane.read(cx);
        let visible_cwd = matches!(scope, GitProbeScope::Local)
            .then(|| infer_terminal_cwd_from_text(&pane.visible_text_snapshot()))
            .flatten();
        let cwd = preferred_git_cwd(scope, snapshot_cwd, visible_cwd)?;
        Some(match scope {
            GitProbeScope::Local => expand_local_git_home(&cwd),
            GitProbeScope::SshNode(_) => cwd,
        })
    }
}

pub(super) async fn run_local_git_probe(cwd: &str) -> GitProbeOutcome {
    let root = match run_local_git_command(cwd, git_repo_root_args()).await {
        Ok(output) => output,
        Err(LocalGitProbeError::GitMissing) => return GitProbeOutcome::GitUnavailable,
        Err(LocalGitProbeError::Timeout) => {
            return GitProbeOutcome::Error(oxideterm_environment::GitProbeError::new(
                "local git probe timed out",
            ));
        }
        Err(LocalGitProbeError::Io) => {
            return GitProbeOutcome::Error(oxideterm_environment::GitProbeError::new(
                "local git probe failed",
            ));
        }
    };
    let branch = run_local_git_command(cwd, git_branch_args())
        .await
        .unwrap_or_else(|_| GitCommandOutput::failure(""));
    let head = run_local_git_command(cwd, git_head_args())
        .await
        .unwrap_or_else(|_| GitCommandOutput::failure(""));
    let status = run_local_git_command(cwd, git_status_args())
        .await
        .unwrap_or_else(|_| GitCommandOutput::failure(""));
    let operation = run_local_git_operation_probe(cwd)
        .await
        .unwrap_or_else(|_| GitCommandOutput::failure(""));

    interpret_git_command_outputs_with_status_and_operation(root, branch, head, status, operation)
}

async fn run_local_git_branch_list(cwd: &str) -> GitBranchListOutcome {
    let branches = match run_local_git_command_with_timeout(
        cwd,
        git_branch_list_args(),
        TERMINAL_GIT_BRANCH_LIST_TIMEOUT,
    )
    .await
    {
        Ok(output) => output,
        Err(LocalGitProbeError::GitMissing) => return GitBranchListOutcome::GitUnavailable,
        Err(LocalGitProbeError::Timeout) => {
            return GitBranchListOutcome::Error(oxideterm_environment::GitProbeError::new(
                "local git branch list timed out",
            ));
        }
        Err(LocalGitProbeError::Io) => {
            return GitBranchListOutcome::Error(oxideterm_environment::GitProbeError::new(
                "local git branch list failed",
            ));
        }
    };
    let worktrees = run_local_git_command_with_timeout(
        cwd,
        git_worktree_list_args(),
        TERMINAL_GIT_BRANCH_LIST_TIMEOUT,
    )
    .await
    .unwrap_or_else(|_| GitCommandOutput::failure(""));

    interpret_git_branch_list_outputs(branches, worktrees)
}

async fn run_local_git_operation_probe(cwd: &str) -> Result<GitCommandOutput, LocalGitProbeError> {
    let git_dir = run_local_git_command(cwd, git_absolute_git_dir_args()).await?;
    if !git_dir.success {
        return Ok(GitCommandOutput::failure(""));
    }
    let Some(git_dir) = git_dir
        .stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
    else {
        return Ok(GitCommandOutput::success(""));
    };
    // Local operation detection reads only Git's own control files. The command
    // action still runs visibly in the terminal; this probe only chooses the
    // correct continue/abort/skip verb for the active operation type.
    let operation = git_operation_kind_from_git_dir(std::path::Path::new(git_dir))
        .map(GitOperationKind::as_str)
        .unwrap_or("");
    Ok(GitCommandOutput::success(operation))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LocalGitProbeError {
    GitMissing,
    Timeout,
    Io,
}

async fn run_local_git_command(
    cwd: &str,
    args: oxideterm_environment::GitProbeCommandArgs,
) -> Result<GitCommandOutput, LocalGitProbeError> {
    run_local_git_command_with_timeout(cwd, args, TERMINAL_GIT_PROBE_TIMEOUT).await
}

async fn run_local_git_command_with_timeout(
    cwd: &str,
    args: oxideterm_environment::GitProbeCommandArgs,
    timeout: Duration,
) -> Result<GitCommandOutput, LocalGitProbeError> {
    let mut command = Command::new("git");
    configure_local_git_command(&mut command);
    command
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let output = tokio::time::timeout(timeout, command.output())
        .await
        .map_err(|_| LocalGitProbeError::Timeout)?
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                LocalGitProbeError::GitMissing
            } else {
                LocalGitProbeError::Io
            }
        })?;

    Ok(GitCommandOutput {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
    })
}

fn configure_local_git_command(command: &mut Command) {
    #[cfg(windows)]
    {
        // Local Git probes run from the GUI process in the background. Hide the
        // child console so branch/status refreshes do not flash git.exe windows.
        command.creation_flags(TERMINAL_GIT_CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

pub(super) fn terminal_git_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

