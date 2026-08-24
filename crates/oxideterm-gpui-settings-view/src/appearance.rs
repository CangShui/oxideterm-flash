// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Presentational builders for the settings appearance page.
//!
//! This module deliberately accepts already-resolved labels, icons, and callback
//! targets from the app crate. It owns reusable GPUI structure and sizing, while
//! workspace state transitions stay in `oxideterm-gpui-app`.

use gpui::{
    AnyElement, Div, IntoElement, ObjectFit, ParentElement, Styled, div, img, prelude::*, px,
    relative, rgb, rgba,
};
use oxideterm_i18n::I18n;
use oxideterm_settings::PersistedSettings;
use oxideterm_theme::ThemeTokens;

const SETTINGS_BG_ACTIVE_SURFACE_ALPHA: u32 = 0x66;
const BACKGROUND_THUMBNAIL_ASPECT_RATIO: f32 = 16.0 / 9.0;
const BACKGROUND_GALLERY_COLUMNS: f32 = 4.0;

pub fn settings_appearance_card_shell(
    tokens: &ThemeTokens,
    background_active: bool,
    header: AnyElement,
    rows: Vec<AnyElement>,
) -> AnyElement {
    // Appearance cards share the same Tauri card surface treatment as the rest
    // of settings, including translucent mode when the settings background is active.
    let card = div()
        .w_full()
        .min_w(px(0.0))
        .rounded(px(tokens.radii.lg))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .p(px(tokens.metrics.settings_card_padding))
        .flex()
        .flex_col()
        .gap(px(tokens.metrics.settings_card_gap))
        .child(header)
        .children(rows);
    oxideterm_gpui_ui::theme_card_surface(
        card,
        tokens,
        background_active,
        SETTINGS_BG_ACTIVE_SURFACE_ALPHA,
    )
    .into_any_element()
}

pub fn settings_appearance_card_title(
    tokens: &ThemeTokens,
    title: String,
    icon: Option<AnyElement>,
) -> AnyElement {
    // The title builder takes a rendered icon so the app crate can keep owning
    // its Lucide asset enum without leaking that type into this view crate.
    let title_el = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .text_size(px(tokens.metrics.ui_text_sm))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(rgb(tokens.ui.text))
        .when_some(icon, |title, icon| title.child(icon));
    title_el.child(title.to_uppercase()).into_any_element()
}

pub fn settings_appearance_card_header(
    tokens: &ThemeTokens,
    title: String,
    icon: Option<AnyElement>,
    actions: Option<AnyElement>,
) -> AnyElement {
    // Header layout is presentational; callers supply rendered icons/actions
    // so app-specific asset and event types do not cross this crate boundary.
    div()
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .child(settings_appearance_card_title(tokens, title, icon))
        .when_some(actions, |header, actions| header.child(actions))
        .into_any_element()
}

pub fn settings_appearance_row(
    tokens: &ThemeTokens,
    i18n: &I18n,
    label_key: &str,
    hint_key: &str,
    control: AnyElement,
) -> AnyElement {
    // Rows are pure label/hint/control layout; callers decide which control
    // handles focus, mutation, or async work.
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(tokens.metrics.settings_row_gap))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(tokens.ui.text))
                        .child(i18n.t(label_key)),
                )
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_xs))
                        .text_color(rgb(tokens.ui.text_muted))
                        .child(i18n.t(hint_key)),
                ),
        )
        .child(control)
        .into_any_element()
}

pub fn settings_appearance_radius_control(
    tokens: &ThemeTokens,
    radius: i64,
    slider: AnyElement,
) -> AnyElement {
    // Radius preview is display-only. The app supplies the slider because drag
    // state and settings mutation live at the workspace boundary.
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .child(
            div()
                .size(px(28.0))
                .rounded(px(radius as f32))
                .border_1()
                .border_color(rgb(tokens.ui.border))
                .bg(rgb(tokens.ui.bg_secondary)),
        )
        .child(slider)
        .child(
            div()
                .w(px(48.0))
                .text_align(gpui::TextAlign::Right)
                .text_size(px(tokens.metrics.ui_text_xs))
                .text_color(rgb(tokens.ui.text_muted))
                .child(format!("{radius}px")),
        )
        .into_any_element()
}

pub fn settings_appearance_theme_preview(
    tokens: &ThemeTokens,
    settings: &PersistedSettings,
) -> AnyElement {
    // This is a static terminal sample, so it can live outside WorkspaceApp
    // without knowing anything about panes, sessions, or live terminal state.
    let terminal = tokens.terminal;
    div()
        .w_full()
        .mt(px(tokens.metrics.settings_font_preview_margin_top))
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .bg(rgb(terminal.background))
        .p(px(tokens.metrics.settings_theme_preview_padding))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .flex()
                .flex_row()
                .gap(px(tokens.metrics.settings_theme_preview_dot_gap))
                .child(settings_appearance_preview_dot(
                    terminal.red,
                    tokens.metrics.settings_theme_preview_dot_size,
                ))
                .child(settings_appearance_preview_dot(
                    terminal.yellow,
                    tokens.metrics.settings_theme_preview_dot_size,
                ))
                .child(settings_appearance_preview_dot(
                    terminal.green,
                    tokens.metrics.settings_theme_preview_dot_size,
                )),
        )
        .child(
            div()
                .font_family(
                    settings
                        .terminal
                        .font_family
                        .terminal_family_name(&settings.terminal.custom_font_family),
                )
                .text_size(px(tokens.metrics.ui_text_xs))
                .line_height(px(tokens.metrics.settings_theme_preview_line_height))
                .text_color(rgb(terminal.foreground))
                .flex()
                .flex_col()
                .child("$ echo \"Hello World\"")
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(6.0))
                        .child(div().text_color(rgb(terminal.blue)).child("~"))
                        .child(div().text_color(rgb(terminal.magenta)).child("git"))
                        .child(div().text_color(rgb(terminal.blue)).child("status")),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(4.0))
                        .child(">")
                        .child(div().w(px(9.0)).h(px(18.0)).bg(rgb(terminal.cursor))),
                ),
        )
        .into_any_element()
}

pub fn settings_background_thumbnails_layout(thumbnails: Vec<AnyElement>) -> AnyElement {
    // GPUI's grid row measurement can overestimate aspect-ratio children from
    // full container width, so this mirrors Tauri's quarter-width slot manually.
    let mut layout = div().w_full().flex().flex_row().flex_wrap().gap(px(8.0));
    for thumbnail in thumbnails {
        layout = layout.child(
            div()
                .w(relative(1.0 / BACKGROUND_GALLERY_COLUMNS))
                .flex_none()
                .child(thumbnail),
        );
    }
    layout.into_any_element()
}

pub fn settings_background_gallery(
    tokens: &ThemeTokens,
    title: String,
    actions: AnyElement,
    thumbnails: AnyElement,
) -> AnyElement {
    // The gallery shell owns spacing and title/action placement. File picking
    // and clear-all behavior stay in the app-provided action elements.
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .w_full()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(tokens.ui.text))
                        .child(title),
                )
                .child(actions),
        )
        .child(thumbnails)
        .into_any_element()
}

pub fn settings_background_empty_hint(tokens: &ThemeTokens, label: String) -> AnyElement {
    // Empty gallery text is just display state; the app decides when no
    // background image exists.
    div()
        .text_size(px(tokens.metrics.ui_text_xs))
        .text_color(rgb(tokens.ui.text_muted))
        .child(label)
        .into_any_element()
}

pub fn settings_background_thumbnail_frame(
    tokens: &ThemeTokens,
    image_path: &str,
    active: bool,
    active_label: String,
    image_fallback_icon: impl Fn() -> AnyElement + 'static,
) -> Div {
    // The frame owns crop, fallback, active border, and badge. Select/remove
    // mouse handlers are intentionally attached by the app after construction.
    let image_source = std::path::PathBuf::from(image_path);
    let fallback_label = std::path::Path::new(image_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(image_path)
        .to_string();
    let fallback_text_size = tokens.metrics.ui_text_xs;
    let fallback_text_color = tokens.ui.text_muted;
    let fallback_bg = tokens.ui.bg_sunken;
    let thumbnail_radius = tokens.radii.md;
    let image = img(image_source)
        .w_full()
        .h_full()
        .object_fit(ObjectFit::Cover)
        // Tauri uses `rounded-md overflow-hidden` on the thumbnail wrapper.
        // GPUI does not clip image children through that wrapper reliably, so
        // the image owns the same rounded mask as the frame.
        .rounded(px(thumbnail_radius))
        .with_fallback(move || {
            div()
                .w_full()
                .h_full()
                .rounded(px(thumbnail_radius))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(6.0))
                .bg(rgb(fallback_bg))
                .child(image_fallback_icon())
                .child(
                    div()
                        .max_w_full()
                        .px(px(8.0))
                        .text_size(px(fallback_text_size))
                        .text_color(rgb(fallback_text_color))
                        .truncate()
                        .child(fallback_label.clone()),
                )
                .into_any_element()
        });

    let mut thumbnail = div()
        .relative()
        .w_full()
        .rounded(px(thumbnail_radius))
        .overflow_hidden()
        .border_2()
        .border_color(rgb(if active {
            tokens.ui.accent
        } else {
            tokens.ui.border
        }))
        .cursor_pointer()
        // Keep the crop owned by the wrapper so the rounded border and image
        // match the browser BackgroundImageSection thumbnail.
        .child(image);
    thumbnail.style().aspect_ratio = Some(BACKGROUND_THUMBNAIL_ASPECT_RATIO);
    thumbnail.when(active, |thumb| {
        thumb.child(
            div()
                .absolute()
                .top(px(8.0))
                .left(px(8.0))
                .rounded(px(tokens.radii.sm))
                .bg(rgb(tokens.ui.accent))
                .px(px(tokens.metrics.settings_background_badge_padding_x))
                .py(px(tokens.metrics.settings_background_badge_padding_y))
                .text_size(px(tokens.metrics.ui_text_xs))
                .text_color(rgb(tokens.ui.accent_text))
                .child(active_label),
        )
    })
}

pub fn settings_background_clear_all_button(
    tokens: &ThemeTokens,
    label: String,
    icon: AnyElement,
) -> Div {
    // Clear-all uses destructive styling, but the actual destructive mutation
    // is attached by the app after this visual builder returns.
    div()
        .h(px(tokens.metrics.settings_appearance_action_height))
        .px(px(10.0))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .rounded(px(tokens.radii.md))
        .text_size(px(tokens.metrics.ui_text_xs))
        .text_color(rgb(tokens.ui.error))
        .cursor_pointer()
        .hover(|style| style.bg(rgba((tokens.ui.error << 8) | 0x14)))
        .child(icon)
        .child(label)
}

pub fn settings_background_thumbnail_remove_button(
    tokens: &ThemeTokens,
    close_icon: AnyElement,
) -> Div {
    // The close button is visual chrome only; callers attach the destructive
    // intent so file and gallery state remain in the owning settings Entity.
    div()
        .absolute()
        .top(px(6.0))
        .right(px(6.0))
        .p(px(3.0))
        .rounded(px(tokens.radii.sm))
        .bg(rgba(0x00000099))
        .text_color(rgb(tokens.ui.text))
        .child(close_icon)
}

pub fn settings_background_tabs_section(
    tokens: &ThemeTokens,
    title: String,
    hint: String,
    pills: Vec<AnyElement>,
) -> AnyElement {
    // The tabs section owns the three-column visual layout. Callers build each
    // pill with app-owned toggle handlers.
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(tokens.ui.text))
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_xs))
                        .text_color(rgb(tokens.ui.text_muted))
                        .child(hint),
                ),
        )
        .child(
            div()
                .w_full()
                .grid()
                .grid_cols(3)
                .gap(px(10.0))
                .children(pills),
        )
        .into_any_element()
}

pub fn settings_background_tab_pill(
    tokens: &ThemeTokens,
    label: String,
    icon: AnyElement,
    enabled: bool,
) -> Div {
    // Background tab pills are static option rows. The app supplies translated
    // labels and rendered icons, then wires click-to-toggle behavior.
    div()
        .h(px(40.0))
        .min_w(px(0.0))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(10.0))
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgb(if enabled {
            tokens.ui.accent
        } else {
            tokens.ui.border
        }))
        .bg(if enabled {
            rgba((tokens.ui.accent << 8) | 0x1a)
        } else {
            rgba(0x00000000)
        })
        .px(px(14.0))
        .text_size(px(tokens.metrics.ui_text_sm))
        .text_color(rgb(if enabled {
            tokens.ui.accent
        } else {
            tokens.ui.text_muted
        }))
        .cursor_pointer()
        .child(icon)
        .child(div().truncate().child(label))
}

fn settings_appearance_preview_dot(color: u32, size: f32) -> AnyElement {
    div()
        .size(px(size))
        .rounded_full()
        .bg(rgb(color))
        .into_any_element()
}
