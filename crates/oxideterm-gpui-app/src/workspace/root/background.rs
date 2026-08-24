use super::super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn render_workspace_window_background(
        &mut self,
        window_background: &Entity<window_shell::WorkspaceWindowBackgroundEntity>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let background = self.window_background_preferences()?;
        Some(self.render_workspace_background_layer(window_background, background, window, cx))
    }

    pub(in crate::workspace) fn wrap_content_background(
        &mut self,
        window_background: &Entity<window_shell::WorkspaceWindowBackgroundEntity>,
        content: AnyElement,
        background_key: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(background_key) = background_key else {
            return content;
        };
        if matches!(background_key, "terminal" | "local_terminal") {
            return content;
        }
        let Some(background) = self.terminal_background_preferences(background_key) else {
            return content;
        };
        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .child(self.render_workspace_background_layer(
                window_background,
                background,
                window,
                cx,
            ))
            .child(div().relative().size_full().child(content))
            .into_any_element()
    }

    fn render_workspace_background_layer(
        &mut self,
        window_background: &Entity<window_shell::WorkspaceWindowBackgroundEntity>,
        background: TerminalBackgroundPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let byte_limit = self.render_policy.image_cache_bytes;
        window_background.update(cx, |window_background, cx| {
            window_background.cache.set_byte_limit(byte_limit);
            window_background.render_layer(background, window, cx)
        })
    }
}

impl window_shell::WorkspaceWindowBackgroundEntity {
    fn render_layer(
        &mut self,
        background: TerminalBackgroundPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let blurred_image = self.cache.render_blurred_image(&background);
        self.drop_retired_images(Some(window), cx);
        if self.cache.has_pending() {
            self.schedule_decode_completion(cx);
        }
        workspace_background_image_layer(background, blurred_image)
    }

    fn schedule_decode_completion(&mut self, cx: &mut Context<Self>) {
        if self.decode_completion_task.is_some() {
            return;
        }
        // Each shell owns its cache completion task, so releasing one native
        // window cannot keep repaint work alive through the shared session.
        self.decode_completion_task = Some(cx.spawn(async move |window_background, cx| {
            Timer::after(Duration::from_millis(16)).await;
            let _ = window_background.update(cx, |window_background, cx| {
                window_background.decode_completion_task = None;
                if window_background.cache.drain_completed() {
                    window_background.drop_retired_images(None, cx);
                    cx.notify();
                }
                if window_background.cache.has_pending() {
                    window_background.schedule_decode_completion(cx);
                }
            });
        }));
    }

    fn drop_retired_images(&mut self, mut window: Option<&mut Window>, cx: &mut Context<Self>) {
        for image in self.cache.take_retired_images() {
            // RenderImage entries painted by gpui::img also stay in the atlas
            // until the app explicitly drops their image id.
            if let Some(window) = window.as_mut() {
                cx.drop_image(image, Some(*window));
            } else {
                cx.drop_image(image, None);
            }
        }
    }
}

pub(in crate::workspace) fn workspace_background_image_layer(
    background: TerminalBackgroundPreferences,
    blurred_image: Option<Arc<RenderImage>>,
) -> AnyElement {
    let image = if let Some(blurred_image) = blurred_image {
        gpui::img(blurred_image)
            .size_full()
            .object_fit(workspace_background_object_fit(background.fit))
            .opacity(background.opacity.clamp(0.0, 1.0))
            .into_any_element()
    } else {
        gpui::img(background.path)
            .size_full()
            .object_fit(workspace_background_object_fit(background.fit))
            .opacity(background.opacity.clamp(0.0, 1.0))
            .with_fallback(|| div().size_full().into_any_element())
            .into_any_element()
    };

    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .overflow_hidden()
        .child(image)
        .into_any_element()
}

pub(in crate::workspace) fn workspace_background_object_fit(
    fit: TerminalBackgroundFit,
) -> ObjectFit {
    match fit {
        TerminalBackgroundFit::Cover => ObjectFit::Cover,
        TerminalBackgroundFit::Contain => ObjectFit::Contain,
        TerminalBackgroundFit::Fill => ObjectFit::Fill,
        TerminalBackgroundFit::Tile => ObjectFit::None,
    }
}

pub(in crate::workspace) fn default_connections_path() -> PathBuf {
    default_settings_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("connections.json")
}

pub(in crate::workspace) fn default_saved_forwards_path() -> PathBuf {
    default_settings_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("forwards.json")
}

pub(in crate::workspace) fn default_session_tree_path() -> PathBuf {
    default_settings_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("session_tree.json")
}

pub(in crate::workspace) fn default_ai_conversations_path() -> PathBuf {
    default_settings_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("chat_history.redb")
}
