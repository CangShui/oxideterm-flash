use super::*;

impl WorkspaceApp {
    pub(in crate::workspace::sftp) fn render_sftp_context_menu(
        &self,
        menu: SftpContextMenu,
        window: &Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let viewport = window.viewport_size();
        let placement = browser_behavior::clamp_context_menu_position(
            menu.x,
            menu.y,
            f32::from(viewport.width),
            f32::from(viewport.height),
            SFTP_CONTEXT_MENU_WIDTH,
            SFTP_CONTEXT_MENU_MAX_HEIGHT,
            8.0,
        );
        let selected_count = self.sftp_selected_names(menu.pane, cx).len();
        let remote_paste_available = menu.pane == SftpPane::Remote
            && self
                .sftp_view
                .read(cx)
                .remote_clipboard
                .as_ref()
                .is_some_and(|clipboard| {
                    self.visible_sftp_remote_id(cx).as_ref() == Some(&clipboard.remote_id)
                });
        let favorite_state = menu
            .file
            .as_ref()
            .filter(|_| menu.pane == SftpPane::Remote)
            .filter(|_| self.active_sftp_saved_connection_id(cx).is_some())
            .map(|file| {
                let path = if file.path.is_empty() {
                    join_sftp_path(&self.sftp_view.read(cx).remote_path, &file.name)
                } else {
                    file.path.clone()
                };
                self.active_sftp_path_is_favorite(&path, cx)
            });
        let (remote_loading, pair_primary_loading) = {
            let sftp = self.sftp_view.read(cx);
            (sftp.remote_loading, sftp.pair_primary_loading)
        };
        let pane_loading = match menu.pane {
            SftpPane::Local => pair_primary_loading,
            SftpPane::Remote => remote_loading,
        };
        let transfer_loading = remote_loading || pair_primary_loading;
        let direction = if menu.pane == SftpPane::Local {
            SftpTransferDirection::Upload
        } else {
            SftpTransferDirection::Download
        };
        let transfer_label = if menu.pane == SftpPane::Local {
            self.i18n.t("sftp.context.upload")
        } else {
            self.i18n.t("sftp.context.download")
        };
        let popup = context_menu_event_boundary(
            div()
                .w(px(SFTP_CONTEXT_MENU_WIDTH))
                .p(px(SFTP_CONTEXT_MENU_PADDING))
                .rounded(px(self.tokens.radii.sm))
                .border_1()
                .border_color(sftp_border(theme.border, has_background))
                .bg(sftp_panel_bg(theme.bg_elevated, has_background, 0xf2))
                .shadow_lg(),
        )
        .when(selected_count > 0, |menu_el| {
            menu_el.child(self.render_sftp_context_menu_guarded_item(
                if menu.pane == SftpPane::Local {
                    LucideIcon::Upload
                } else {
                    LucideIcon::Download
                },
                transfer_label,
                false,
                false,
                transfer_loading,
                has_background,
                move |this, _event, _window, cx| {
                    this.queue_sftp_transfers(menu.pane, direction, cx);
                },
                cx,
            ))
        })
        .when_some(menu.file.clone(), |menu_el, file| {
            if menu.pane != SftpPane::Remote || file.file_type == SftpFileType::Directory {
                menu_el
            } else {
                let can_extract =
                    selected_count == 1 && sftp_extract_archive_kind(&file.name).is_some();
                menu_el
                    .child(self.render_sftp_context_menu_guarded_item(
                        LucideIcon::Pencil,
                        self.i18n.t("sftp.context.edit"),
                        false,
                        false,
                        pane_loading,
                        has_background,
                        {
                            let file = file.clone();
                            move |this, _event, _window, cx| {
                                this.open_or_preview_sftp_file(menu.pane, &file, cx);
                            }
                        },
                        cx,
                    ))
                    .when(can_extract, |menu_el| {
                        menu_el.child(self.render_sftp_context_menu_guarded_item(
                            LucideIcon::FolderArchive,
                            self.i18n.t("sftp.context.extract"),
                            false,
                            false,
                            pane_loading,
                            has_background,
                            {
                                let file = file.clone();
                                move |this, _event, _window, cx| {
                                    this.extract_remote_sftp_archive(file.clone(), cx);
                                    cx.notify();
                                }
                            },
                            cx,
                        ))
                    })
            }
        })
        .when(menu.file.is_some() && selected_count == 1, |menu_el| {
            menu_el.child(self.render_sftp_context_menu_guarded_item(
                LucideIcon::Pencil,
                self.i18n.t("sftp.context.rename"),
                false,
                false,
                pane_loading,
                has_background,
                {
                    let file = menu.file.clone();
                    move |this, _event, _window, cx| {
                        if let Some(file) = file.as_ref() {
                            this.sftp_view.update(cx, |sftp, cx| {
                                sftp.open_rename_dialog(menu.pane, file.name.clone(), cx);
                            });
                        }
                    }
                },
                cx,
            ))
        })
        .when(
            menu.pane == SftpPane::Remote && selected_count > 0,
            |menu_el| {
                menu_el
                    .child(self.render_sftp_context_menu_guarded_item(
                        LucideIcon::FolderArchive,
                        self.i18n.t("sftp.archive.create"), false, false, pane_loading, has_background,
                        |this, _event, _window, cx| this.open_remote_archive_dialog(false, None, cx), cx,
                    ))
                    .child(self.render_sftp_context_menu_guarded_item(
                        LucideIcon::FolderArchive,
                        self.i18n.t("sftp.archive.extract"), false, false, pane_loading, has_background,
                        |this, _event, _window, cx| this.open_remote_archive_dialog(true, None, cx), cx,
                    ))
                    .child(self.render_sftp_context_menu_guarded_item(
                        LucideIcon::Copy,
                        self.i18n.t("menu.copy"),
                        false,
                        false,
                        pane_loading,
                        has_background,
                        move |this, _event, _window, cx| {
                            this.set_remote_sftp_clipboard(SftpRemoteClipboardOperation::Copy, cx);
                        },
                        cx,
                    ))
                    .child(self.render_sftp_context_menu_guarded_item(
                        LucideIcon::Scissors,
                        self.i18n.t("menu.cut"),
                        false,
                        false,
                        pane_loading,
                        has_background,
                        move |this, _event, _window, cx| {
                            this.set_remote_sftp_clipboard(SftpRemoteClipboardOperation::Cut, cx);
                        },
                        cx,
                    ))
            },
        )
        .when(menu.pane == SftpPane::Remote, |menu_el| {
            menu_el.child(self.render_sftp_context_menu_guarded_item(
                LucideIcon::FolderInput,
                self.i18n.t("menu.paste"),
                false,
                !remote_paste_available,
                pane_loading,
                has_background,
                move |this, _event, _window, cx| {
                    this.paste_remote_sftp_clipboard(cx);
                },
                cx,
            ))
        })
        .when_some(menu.file.clone(), |menu_el, file| {
            menu_el.child(self.render_sftp_context_menu_guarded_item(
                LucideIcon::Copy,
                self.i18n.t("sftp.context.copy_path"),
                false,
                false,
                pane_loading,
                has_background,
                move |this, _event, _window, cx| {
                    let base = {
                        let sftp = this.sftp_view.read(cx);
                        match menu.pane {
                            SftpPane::Local => sftp.local_path.clone(),
                            SftpPane::Remote => sftp.remote_path.clone(),
                        }
                    };
                    cx.write_to_clipboard(ClipboardItem::new_string(join_sftp_path(
                        &base, &file.name,
                    )));
                },
                cx,
            ))
        })
        .when_some(favorite_state, |menu_el, is_favorite| {
            let file = menu
                .file
                .clone()
                .expect("favorite state requires a context-menu file");
            menu_el.child(self.render_sftp_context_menu_guarded_item(
                LucideIcon::Star,
                self.i18n.t(if is_favorite {
                    "sftp.context.remove_favorite"
                } else {
                    "sftp.context.add_favorite"
                }),
                false,
                false,
                pane_loading,
                has_background,
                move |this, _event, _window, cx| {
                    this.toggle_active_sftp_path_favorite(file.clone(), cx);
                },
                cx,
            ))
        })
        .when(selected_count > 0, |menu_el| {
            menu_el.child(self.render_sftp_context_menu_guarded_item(
                LucideIcon::Trash2,
                self.i18n.t("sftp.context.delete"),
                true,
                false,
                pane_loading,
                has_background,
                move |this, _event, _window, cx| {
                    let files = this.sftp_selected_names(menu.pane, cx);
                    this.sftp_view.update(cx, |sftp, cx| {
                        sftp.set_dialog(SftpDialog::Delete {
                            pane: menu.pane,
                            files,
                        });
                        cx.notify();
                    });
                },
                cx,
            ))
        })
        .child(
            div()
                .h(px(1.0))
                .my(px(SFTP_CONTEXT_MENU_PADDING))
                .bg(sftp_border(theme.border, has_background)),
        )
        .child(self.render_sftp_context_menu_guarded_item(
            LucideIcon::FolderOpen,
            self.i18n.t("sftp.context.new_folder"),
            false,
            false,
            pane_loading,
            has_background,
            move |this, _event, _window, cx| {
                this.sftp_view.update(cx, |sftp, cx| {
                    sftp.open_new_folder_dialog(menu.pane, cx);
                });
            },
            cx,
        ))
        .child(self.render_sftp_context_menu_guarded_item(
            LucideIcon::FilePlus,
            self.i18n.t("sftp.context.new_file"),
            false,
            false,
            pane_loading,
            has_background,
            move |this, _event, _window, cx| {
                this.sftp_view.update(cx, |sftp, cx| {
                    sftp.open_new_file_dialog(menu.pane, cx);
                });
            },
            cx,
        ));

        self.workspace_context_menu_backdrop(
            deferred(
                anchored()
                    .anchor(Corner::TopLeft)
                    .position(gpui::point(px(placement.x), px(placement.y)))
                    .position_mode(AnchoredPositionMode::Window)
                    .child(overlay_content_boundary(popup)),
            )
            .with_priority(oxideterm_gpui_ui::modal::TAURI_POPOVER_LAYER_PRIORITY),
            cx,
        )
        .into_any_element()
    }

    fn render_sftp_context_menu_guarded_item(
        &self,
        icon: LucideIcon,
        label: String,
        danger: bool,
        disabled: bool,
        loading: bool,
        has_background: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let disabled = disabled
            || self.sftp_view.read(cx).context_menu_presence.phase()
                == oxideterm_gpui_ui::motion::ExitPhase::Exiting;
        let color = if danger { SFTP_RED } else { theme.text };
        let item = div()
            .h(px(SFTP_CONTEXT_MENU_ITEM_HEIGHT))
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(6.0))
            .rounded(px(self.tokens.radii.xs))
            .text_size(px(SFTP_TEXT_XS))
            .text_color(rgb(color))
            .child(Self::render_lucide_icon(icon, SFTP_ICON_SM, rgb(color)))
            .child(div().truncate().child(self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "sftp-context-menu",
                label.clone(),
                label,
                color,
                cx,
            )));
        // SFTP remote refresh/transfer can leave a context menu visible while
        // the backing pane is loading. Route those rows through the shared menu
        // guard so the UI cannot dispatch stale actions.
        self.workspace_context_menu_styled_action(
            item,
            disabled,
            loading,
            ContextMenuActionableStyle {
                hover_background: Some(sftp_hover_bg(theme.bg_hover, has_background)),
                hover_text_color: None,
            },
            |_| {},
            // The shared workspace menu helper already wraps this closure in
            // `cx.listener`. Passing another listener here re-enters WorkspaceApp
            // during the same mouse event and panics on menu actions like Preview.
            move |this, event, window, cx| {
                this.sftp_view
                    .update(cx, |sftp, cx| sftp.dismiss_context_menu(cx));
                listener(this, event, window, cx);
            },
            cx,
        )
        .into_any_element()
    }
}
