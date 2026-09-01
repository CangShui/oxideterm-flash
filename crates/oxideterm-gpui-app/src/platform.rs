use gpui::{Menu, MenuItem, SystemMenuType};
pub use oxideterm_gpui_platform::{window_options, window_options_with_bounds};
use oxideterm_i18n::I18n;

use crate::{
    CloseOtherTabs, ClosePane, CloseTab, CommandPalette, Copy, Cut, Find, FindNext, FindPrev,
    FontDecrease, FontIncrease, FontReset, NewConnection, NewTerminal, NextTab, OpenSettings,
    PaletteBroadcast, PaletteCancelReconnect, PaletteCleanupDead,
    PaletteDetachTerminal, PaletteDisconnectAll, PaletteHealthCheck,
    PaletteReconnectAll, PaletteResetPanes, Paste, PrevTab, Quit, ShellLauncher,
    SplitHorizontal, SplitVertical, TerminalRecording, ToggleSidebar, ZenMode,
};

pub(crate) fn app_menus(i18n: &I18n) -> Vec<Menu> {
    vec![
        Menu {
            disabled: false,
            name: i18n.t("menu.app").into(),
            items: vec![
                MenuItem::os_submenu(i18n.t("menu.services"), SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action(i18n.t("command_palette.title"), CommandPalette),
                MenuItem::action(i18n.t("menu.settings"), OpenSettings),
                MenuItem::separator(),
                MenuItem::action(i18n.t("menu.quit"), Quit),
            ],
        },
        Menu {
            disabled: false,
            name: i18n.t("menu.edit").into(),
            items: vec![
                MenuItem::action(i18n.t("menu.cut"), Cut),
                MenuItem::action(i18n.t("menu.copy"), Copy),
                MenuItem::action(i18n.t("menu.paste"), Paste),
                MenuItem::separator(),
                MenuItem::action(i18n.t("menu.find"), Find),
                MenuItem::action(i18n.t("menu.find_next"), FindNext),
                MenuItem::action(i18n.t("menu.find_previous"), FindPrev),
            ],
        },
        Menu {
            disabled: false,
            name: i18n.t("menu.terminal").into(),
            items: vec![
                MenuItem::action(i18n.t("command_palette.cmd_new_terminal"), NewTerminal),
                MenuItem::action(i18n.t("command_palette.cmd_shell_launcher"), ShellLauncher),
                MenuItem::action(i18n.t("command_palette.cmd_new_connection"), NewConnection),
                MenuItem::separator(),
                MenuItem::action(i18n.t("menu.split_horizontal"), SplitHorizontal),
                MenuItem::action(i18n.t("menu.split_vertical"), SplitVertical),
                MenuItem::action(i18n.t("menu.close_pane"), ClosePane),
                MenuItem::separator(),
                MenuItem::action(
                    i18n.t("command_palette.cmd_broadcast_toggle"),
                    PaletteBroadcast,
                ),
                MenuItem::action(
                    i18n.t("terminal.recording.title"),
                    TerminalRecording,
                ),
                MenuItem::action(
                    i18n.t("command_palette.cmd_detach_terminal"),
                    PaletteDetachTerminal,
                ),
                MenuItem::action(
                    i18n.t("command_palette.cmd_cleanup_dead"),
                    PaletteCleanupDead,
                ),
                MenuItem::separator(),
                MenuItem::action(i18n.t("command_palette.cmd_reset_panes"), PaletteResetPanes),
            ],
        },
        Menu {
            disabled: false,
            name: i18n.t("menu.view").into(),
            items: vec![
                MenuItem::action(i18n.t("command_palette.title"), CommandPalette),
                MenuItem::action(i18n.t("command_palette.cmd_toggle_sidebar"), ToggleSidebar),
                MenuItem::separator(),
                MenuItem::action(i18n.t("command_palette.cmd_font_increase"), FontIncrease),
                MenuItem::action(i18n.t("command_palette.cmd_font_decrease"), FontDecrease),
                MenuItem::action(i18n.t("command_palette.cmd_font_reset"), FontReset),
                MenuItem::separator(),
                MenuItem::action(i18n.t("command_palette.cmd_zen_mode"), ZenMode),
            ],
        },
        Menu {
            disabled: false,
            name: i18n.t("command_palette.cmd_sidebar_connections").into(),
            items: vec![
                MenuItem::action(
                    i18n.t("command_palette.cmd_disconnect_all"),
                    PaletteDisconnectAll,
                ),
                MenuItem::action(
                    i18n.t("command_palette.cmd_reconnect_all"),
                    PaletteReconnectAll,
                ),
                MenuItem::action(
                    i18n.t("command_palette.cmd_cancel_reconnect"),
                    PaletteCancelReconnect,
                ),
                MenuItem::separator(),
                MenuItem::action(
                    i18n.t("command_palette.cmd_health_check"),
                    PaletteHealthCheck,
                ),
            ],
        },
        Menu {
            disabled: false,
            name: i18n.t("menu.window").into(),
            items: vec![
                MenuItem::action(i18n.t("menu.close_tab"), CloseTab),
                MenuItem::action(
                    i18n.t("command_palette.cmd_close_other_tabs"),
                    CloseOtherTabs,
                ),
                MenuItem::separator(),
                MenuItem::action(i18n.t("menu.next_tab"), NextTab),
                MenuItem::action(i18n.t("menu.previous_tab"), PrevTab),
            ],
        },
    ]
}
