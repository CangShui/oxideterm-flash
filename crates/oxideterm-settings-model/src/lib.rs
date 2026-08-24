// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Settings page model crate.
//!
//! This crate owns non-GPUI settings page behavior: reconnect option models,
//! semantic scheme helpers, theme editing state, and compact view-model
//! helpers.

pub mod input_draft;
pub mod navigation;
pub mod reconnect;
pub mod semantic_scheme;
pub mod theme;
pub mod types;

pub use input_draft::*;
pub use navigation::*;
pub use reconnect::*;
pub use semantic_scheme::*;
pub use theme::*;
pub use types::*;
