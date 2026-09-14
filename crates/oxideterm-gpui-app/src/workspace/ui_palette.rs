// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Tailwind palette values shared by feature views that mirror the Tauri
//! port's utility classes. The default theme already maps `ui.success`,
//! `ui.warning`, and `ui.error` to the same green/yellow/red, so new code
//! should prefer those theme tokens; these constants exist for shades with
//! no token (blue/orange/purple) and as the single source so features stop
//! re-declaring raw hex per module.

pub(crate) const BLUE_400: u32 = 0x60a5fa;
pub(crate) const GREEN_500: u32 = 0x22c55e;
pub(crate) const GREEN_900: u32 = 0x14532d;
pub(crate) const RED_900: u32 = 0x7f1d1d;
pub(crate) const YELLOW_500: u32 = 0xeab308;
pub(crate) const ORANGE_400: u32 = 0xfb923c;
#[allow(dead_code)]
pub(crate) const PURPLE_400: u32 = 0xc084fc;
pub(crate) const RED_400: u32 = 0xf87171;
pub(crate) const ZINC_400: u32 = 0xa1a1aa;
