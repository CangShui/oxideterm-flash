// Built-in terminal themes. Only the default dark palette remains; it was
// initially ported from the Tauri app's src/lib/themes.ts.

pub const DEFAULT_THEME: BuiltInTheme = BUILT_IN_THEMES[0];

pub const BUILT_IN_THEMES: &[BuiltInTheme] = &[
    BuiltInTheme {
        id: "default",
        terminal: TerminalTheme {
            background: 0x09090b,
            foreground: 0xf4f4f5,
            cursor: 0xea580c,
            selection_background: "rgba(234, 88, 12, 0.3)",
            black: 0x09090b,
            red: 0xef4444,
            green: 0x22c55e,
            yellow: 0xeab308,
            blue: 0x3b82f6,
            magenta: 0xd946ef,
            cyan: 0x06b6d4,
            white: 0xf4f4f5,
            bright_black: 0x71717a,
            bright_red: 0xf87171,
            bright_green: 0x4ade80,
            bright_yellow: 0xfacc15,
            bright_blue: 0x60a5fa,
            bright_magenta: 0xe879f9,
            bright_cyan: 0x22d3ee,
            bright_white: 0xffffff,
        },
    },
];
