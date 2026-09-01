// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    // Keep argument parsing inside the updater crate so the helper binary stays
    // a tiny process boundary around the staged replacement engine.
    if let Err(error) = run_platform_update_helper() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn run_platform_update_helper() -> Result<(), String> {
    oxideterm_update::parse_windows_update_helper_options(std::env::args_os())
        .and_then(oxideterm_update::run_windows_update_helper)
}

#[cfg(not(windows))]
fn run_platform_update_helper() -> Result<(), String> {
    Err("installed-app update helper mode is only available on Windows".to_string())
}
