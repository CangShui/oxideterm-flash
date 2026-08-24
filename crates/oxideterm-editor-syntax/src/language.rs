// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::path::Path;

use tree_sitter::Language;

unsafe extern "C" {
    fn tree_sitter_fish() -> *const tree_sitter::ffi::TSLanguage;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum LanguageId {
    Bash,
    Zsh,
    Fish,
    Powershell,
}

/// Keep the IDE language surface explicit so adding or removing grammars is a
/// conscious product decision instead of an accidental dependency side effect.
pub const SUPPORTED_LANGUAGES: &[LanguageId] = &[
    LanguageId::Bash,
    LanguageId::Zsh,
    LanguageId::Fish,
    LanguageId::Powershell,
];

impl LanguageId {
    pub fn from_path(path: impl AsRef<Path>) -> Option<Self> {
        let path = path.as_ref();
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_ascii_lowercase());
        if let Some(language) = file_name.as_deref().and_then(language_from_known_file_name) {
            return Some(language);
        }
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase());
        match extension.as_deref() {
            Some("bash" | "sh") => Some(Self::Bash),
            Some("fish") => Some(Self::Fish),
            Some("ps1" | "psm1" | "psd1") => Some(Self::Powershell),
            Some("zsh" | "zsh-theme") => Some(Self::Zsh),
            _ => None,
        }
    }

    pub fn detect(path: Option<&Path>, source: &str) -> Option<Self> {
        path.and_then(Self::from_path)
            .or_else(|| language_from_shebang(source))
    }

    pub(crate) fn tree_sitter_language(self) -> Language {
        match self {
            Self::Bash => tree_sitter_bash::LANGUAGE.into(),
            Self::Zsh => tree_sitter_zsh::LANGUAGE.into(),
            Self::Fish => fish_language(),
            Self::Powershell => tree_sitter_powershell::LANGUAGE.into(),
        }
    }

    pub(crate) fn highlight_query(self) -> &'static str {
        crate::queries::highlight_query_for(self)
    }
}

fn language_from_known_file_name(file_name: &str) -> Option<LanguageId> {
    if matches!(
        file_name,
        ".bashrc" | ".bash_profile" | ".bash_login" | ".profile"
    ) {
        return Some(LanguageId::Bash);
    }
    if matches!(
        file_name,
        ".zshrc" | ".zprofile" | ".zshenv" | ".zlogin" | ".zlogout"
    ) {
        return Some(LanguageId::Zsh);
    }
    None
}

fn fish_language() -> Language {
    // `tree-sitter-fish` still exposes the pre-LanguageFn Rust helper, so use
    // the generated C symbol directly to stay on OxideTerm's tree-sitter ABI.
    unsafe { Language::from_raw(tree_sitter_fish()) }
}

fn language_from_shebang(source: &str) -> Option<LanguageId> {
    let first = source.lines().next()?;
    if !first.starts_with("#!") {
        return None;
    }
    let lower = first.to_ascii_lowercase();
    if lower.contains("zsh") {
        return Some(LanguageId::Zsh);
    }
    if lower.contains("bash") || lower.contains("/sh") {
        return Some(LanguageId::Bash);
    }
    if lower.contains("fish") {
        return Some(LanguageId::Fish);
    }
    if lower.contains("pwsh") || lower.contains("powershell") {
        return Some(LanguageId::Powershell);
    }
    None
}
