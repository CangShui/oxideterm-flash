use super::*;

/// Period between Host Tools lifecycle resyncs. Samplers keep their own
/// cadence; this tick only re-applies visibility and selection state.
const HOST_TOOLS_LIFECYCLE_REFRESH_INTERVAL: std::time::Duration =
    std::time::Duration::from_secs(2);

fn host_tools_visibility(sidebar_visible: bool) -> HostToolsVisibility {
    HostToolsVisibility::from_mounts(false, sidebar_visible, false)
}

impl HostToolsEntity {
    pub(super) fn schedule_lifecycle_refresh(&mut self, cx: &mut Context<Self>) {
        self.lifecycle_refresh_task = Some(cx.spawn(async move |host_tools, cx| {
            loop {
                Timer::after(HOST_TOOLS_LIFECYCLE_REFRESH_INTERVAL).await;
                let should_continue = host_tools
                    .update(cx, |host_tools, cx| {
                        host_tools.refresh_lifecycle_tick(cx);
                        true
                    })
                    .unwrap_or(false);
                if !should_continue {
                    break;
                }
            }
        }));
    }

    pub(super) fn refresh_lifecycle_tick(&mut self, cx: &mut Context<Self>) {
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            return;
        };
        if !self.visibility.is_visible() {
            // Hidden pages keep shared nodes and long-running work, but do not
            // sample or repaint page-only snapshots.
            return;
        }
        self.update_lifecycle(
            self.visibility,
            self.monitoring.clone(),
            self.sampling_config,
            runtime,
            cx,
        );
        // Virtual-terminal sessions can begin or end without any local UI
        // action, so refresh this lightweight snapshot while its tool is visible.
        if self.active_tool() == ContextSidebarTool::Tmux {
            self.request_active_tool_snapshot(HostSnapshotFeedback::Silent, cx);
        }
    }

    pub(in crate::workspace) fn set_messages(&mut self, messages: HostToolsMessages) {
        self.messages = Some(messages);
    }

    pub(in crate::workspace) fn update_lifecycle(
        &mut self,
        visibility: HostToolsVisibility,
        monitoring: oxideterm_settings::HostToolsSettings,
        sampling_config: oxideterm_connection_monitor::ResourceSamplingConfig,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) {
        self.visibility = visibility;
        self.monitoring = monitoring;
        self.sampling_config = sampling_config;
        self.lifecycle_runtime = Some(runtime.clone());
        let gpu_visible = self.monitoring.gpu_enabled
            && visibility.sidebar_is_visible()
            && self.active_tool() == ContextSidebarTool::Gpu;
        let selected_connection_id = self.selected_connection_id_owned();
        self.sync_gpu_sampling(gpu_visible, selected_connection_id, runtime.clone(), cx);

        if !visibility.sidebar_is_visible() {
            // Resource samplers belong to the mounted Host Tools sidebar.
            self.stop_profiler_sampling();
            self.pause_service_refreshes();
            return;
        }

        // Always resync: the sync freezes profilers that no longer match the
        // displayed tool or host, so skipping it would keep hidden hosts
        // sampling in the background.
        self.sync_live_connections(
            self.monitor_connections(),
            sampling_config,
            runtime,
            cx,
        );
    }

    pub(super) fn sync_live_connections(
        &mut self,
        connections: Vec<MonitorConnectionOption>,
        sampling_config: oxideterm_connection_monitor::ResourceSamplingConfig,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) {
        let live_connection_ids = connections
            .iter()
            .map(|connection| connection.connection_id.as_str())
            .collect::<HashSet<_>>();
        for connection_id in self.profiler_connection_ids() {
            if !live_connection_ids.contains(connection_id.as_str()) {
                self.remove_profiler_connection(&connection_id);
            }
        }
        // Frozen snapshots must not outlive the host they belong to.
        self.host_tmux
            .snapshots
            .retain(|connection_id, _| live_connection_ids.contains(connection_id.as_str()));
        self.host_tmux
            .screen_snapshots
            .retain(|connection_id, _| live_connection_ids.contains(connection_id.as_str()));
        self.host_logs
            .snapshots
            .retain(|connection_id, _| live_connection_ids.contains(connection_id.as_str()));
        self.host_logs
            .running
            .retain(|connection_id, _| live_connection_ids.contains(connection_id.as_str()));
        self.host_gpu
            .snapshots
            .retain(|connection_id, _| live_connection_ids.contains(connection_id.as_str()));
        if connections.is_empty() {
            if let Some(connection_id) = self.take_selected_connection(cx) {
                self.remove_profiler_connection(&connection_id);
            }
            return;
        }

        let live_connection_ids = connections
            .into_iter()
            .map(|connection| connection.connection_id)
            .collect::<Vec<_>>();
        let Some(connection_id) = self.ensure_selected_connection(&live_connection_ids, cx) else {
            return;
        };
        // Only the profiler tool currently displayed may keep a live sampler,
        // and only for its selected host; every other connection freezes with
        // its last snapshot retained.
        let profiler_tool_active = self.visibility.sidebar_is_visible()
            && !sampling_config.is_empty()
            && matches!(
                self.active_tool(),
                ContextSidebarTool::Monitor
                    | ContextSidebarTool::Processes
                    | ContextSidebarTool::Docker
            );
        for other in self.profiler_connection_ids() {
            if !profiler_tool_active || other != connection_id {
                self.profiler_registry.stop(&other);
            }
        }
        if profiler_tool_active && self.profiler_connection_missing(&connection_id) {
            self.start_profiler(connection_id, sampling_config, runtime, cx);
        }
    }

    pub(in crate::workspace) fn apply_monitoring_settings(
        &mut self,
        visibility: HostToolsVisibility,
        monitoring: oxideterm_settings::HostToolsSettings,
        sampling_config: oxideterm_connection_monitor::ResourceSamplingConfig,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) {
        self.visibility = visibility;
        self.monitoring = monitoring;
        self.sampling_config = sampling_config;
        self.lifecycle_runtime = Some(runtime.clone());
        if !visibility.sidebar_is_visible() {
            self.stop_profiler_sampling();
        } else {
            // The sync owns start/stop policy: only the displayed profiler
            // tool and its selected host keep a live sampler.
            self.sync_live_connections(
                self.monitor_connections(),
                sampling_config,
                runtime.clone(),
                cx,
            );
        }

        let gpu_visible = self.monitoring.gpu_enabled
            && visibility.sidebar_is_visible()
            && self.active_tool() == ContextSidebarTool::Gpu;
        let selected_connection_id = self.selected_connection_id_owned();
        self.sync_gpu_sampling(gpu_visible, selected_connection_id, runtime, cx);
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn host_tools_visibility(&self, cx: &App) -> HostToolsVisibility {
        let sidebar_visible = self.context_sidebar_visible()
            && self.active_context_sidebar_panel == ContextSidebarPanel::HostTools;

        host_tools_visibility(sidebar_visible)
    }

    pub(in crate::workspace) fn sync_host_tools_lifecycle(&mut self, cx: &mut App) {
        let visibility = self.host_tools_visibility(cx);
        let monitoring = self.settings_store.settings().host_tools.clone();
        let sampling_config = self.resource_sampling_config();
        let runtime = self.forwarding_runtime.handle().clone();
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.update_lifecycle(visibility, monitoring, sampling_config, runtime, cx);
        });
    }

    pub(in crate::workspace) fn apply_host_tool_monitoring_settings(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let visibility = self.host_tools_visibility(cx);
        let monitoring = self.settings_store.settings().host_tools.clone();
        let sampling_config = self.resource_sampling_config();
        let runtime = self.forwarding_runtime.handle().clone();
        let messages = HostToolsMessages::from_i18n(&self.i18n);
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.set_messages(messages);
            host_tools.apply_monitoring_settings(
                visibility,
                monitoring,
                sampling_config,
                runtime,
                cx,
            );
        });
    }

    fn resource_sampling_config(&self) -> oxideterm_connection_monitor::ResourceSamplingConfig {
        let host_tools = &self.settings_store.settings().host_tools;
        oxideterm_connection_monitor::ResourceSamplingConfig {
            system: host_tools.monitor_enabled,
            // The detailed GPU page owns its own task; this probe only feeds Monitor summaries.
            gpu: host_tools.monitor_enabled && host_tools.gpu_enabled,
            processes: host_tools.processes_enabled,
            docker: host_tools.docker_enabled,
        }
    }

    pub(super) fn monitor_connections(
        &self,
        cx: &mut Context<Self>,
    ) -> Vec<MonitorConnectionOption> {
        self.host_tools.read(cx).monitor_connections()
    }
}
