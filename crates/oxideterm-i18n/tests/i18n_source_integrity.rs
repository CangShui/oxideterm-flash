//! Source-integrity gate for the locale catalogs.
//!
//! Language-pair equality alone cannot stop "delete the same key from every
//! locale at once": the catalogs stay symmetric while user-visible copy
//! silently degrades to raw key names. This test closes that hole by walking
//! the Rust sources and asserting that every reference — direct `t("...")`
//! calls, dynamically formatted key families (`format!("..._{var}")`), and
//! key-shaped literals returned from helper functions — still resolves to a
//! catalog entry. It also pins the locale set, so removing or adding a whole
//! language pack is a visible decision instead of an accident.
//!
//! This runs on the default `cargo test` path; `scripts/quality/audit_i18n.py`
//! implements the same rules for manual and CI use.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const REQUIRED_LOCALES: [&str; 2] = ["en", "zh-CN"];
/// Sentinel exercised by the fallback tests; never a real catalog key.
const IGNORED_KEYS: [&str; 1] = ["missing.key"];

#[test]
fn catalogs_cover_every_source_reference() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("oxideterm-i18n lives at <workspace>/crates/oxideterm-i18n")
        .to_path_buf();
    let source_root = workspace_root.join("crates");
    let locale_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("locales");

    let catalogs = load_catalogs(&locale_root);
    let mut violations: Vec<String> = Vec::new();

    check_locale_set(&catalogs, &mut violations);
    check_language_pair_equality(&catalogs, &mut violations);

    let usage = collect_source_usage(&source_root);
    check_direct_keys(&catalogs, &usage, &mut violations);
    check_template_families(&catalogs, &usage, &mut violations);
    check_namespace_literals(&catalogs, &usage, &mut violations);

    assert!(
        violations.is_empty(),
        "i18n integrity gate failed:\n{}",
        violations.join("\n")
    );
}

type Catalog = BTreeMap<String, BTreeMap<String, String>>;

fn load_catalogs(locale_root: &Path) -> Catalog {
    let mut catalogs: Catalog = BTreeMap::new();
    let entries = std::fs::read_dir(locale_root).expect("locale root directory must exist");
    for entry in entries {
        let locale_dir = entry.expect("locale entry readable").path();
        if !locale_dir.is_dir() {
            continue;
        }
        let locale = locale_dir
            .file_name()
            .and_then(|name| name.to_str())
            .expect("locale directory name is UTF-8")
            .to_string();
        let mut values = BTreeMap::new();
        let files = std::fs::read_dir(&locale_dir).expect("locale files readable");
        for file in files {
            let path = file.expect("locale file readable").path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let raw = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            let parsed: serde_json::Value = serde_json::from_str(&raw)
                .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()));
            flatten(&parsed, String::new(), &mut values);
        }
        catalogs.insert(locale, values);
    }
    catalogs
}

fn flatten(value: &serde_json::Value, prefix: String, out: &mut BTreeMap<String, String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let next = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten(child, next, out);
            }
        }
        serde_json::Value::String(text) => {
            out.insert(prefix, text.clone());
        }
        _ => {}
    }
}

fn check_locale_set(catalogs: &Catalog, violations: &mut Vec<String>) {
    let found: BTreeSet<&String> = catalogs.keys().collect();
    let expected: BTreeSet<&str> = REQUIRED_LOCALES.into_iter().collect();
    for locale in &expected {
        if !found.iter().any(|name| name == locale) {
            violations.push(format!("required locale missing: {locale}"));
        }
    }
    for locale in found {
        if !expected.contains(locale.as_str()) {
            violations.push(format!("unexpected locale directory: {locale}"));
        }
    }
}

fn check_language_pair_equality(catalogs: &Catalog, violations: &mut Vec<String>) {
    let keys: Vec<BTreeSet<&String>> = catalogs.values().map(|map| map.keys().collect()).collect();
    if keys.windows(2).any(|pair| pair[0] != pair[1]) {
        violations.push(
            "locale catalogs diverge: every locale must define the same key set"
                .to_string(),
        );
    }
}

fn check_direct_keys(catalogs: &Catalog, usage: &SourceUsage, violations: &mut Vec<String>) {
    let all_keys: BTreeSet<&String> = catalogs.values().flat_map(|m| m.keys()).collect();
    for (key, files) in &usage.direct {
        if IGNORED_KEYS.contains(&key.as_str()) {
            continue;
        }
        if !all_keys.contains(key) {
            violations.push(format!(
                "missing key referenced by t(): {key} <- {}",
                short_files(files)
            ));
        }
    }
}

fn check_template_families(catalogs: &Catalog, usage: &SourceUsage, violations: &mut Vec<String>) {
    let all_keys: Vec<&String> = catalogs.values().flat_map(|m| m.keys()).collect();
    let roots: BTreeSet<&str> = all_keys
        .iter()
        .map(|key| key.split('.').next().expect("split yields a segment"))
        .collect();
    for (prefix, files) in &usage.template_prefixes {
        let root = prefix.split('.').next().expect("split yields a segment");
        if !roots.contains(root) {
            // Non-i18n template (keychain service ids, config paths) — skip.
            continue;
        }
        if !all_keys.iter().any(|key| key.starts_with(prefix.as_str())) {
            violations.push(format!(
                "formatted key family has no catalog keys: {prefix}* <- {}",
                short_files(files)
            ));
        }
    }
}

fn check_namespace_literals(
    catalogs: &Catalog,
    usage: &SourceUsage,
    violations: &mut Vec<String>,
) {
    let all_keys: BTreeSet<&String> = catalogs.values().flat_map(|m| m.keys()).collect();
    let families: BTreeSet<String> = all_keys
        .iter()
        .filter_map(|key| {
            let segments: Vec<&str> = key.split('.').collect();
            (segments.len() >= 3).then(|| format!("{}.{}", segments[0], segments[1]))
        })
        .collect();
    for (literal, files) in &usage.namespace_literals {
        let segments: Vec<&str> = literal.split('.').collect();
        if segments.len() < 3 {
            continue;
        }
        let family = format!("{}.{}", segments[0], segments[1]);
        if families.contains(&family) && !all_keys.contains(literal) {
            violations.push(format!(
                "missing key referenced as a namespaced literal: {literal} <- {}",
                short_files(files)
            ));
        }
    }
}

fn short_files(files: &BTreeSet<String>) -> String {
    files
        .iter()
        .take(3)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Default)]
struct SourceUsage {
    direct: BTreeMap<String, BTreeSet<String>>,
    template_prefixes: BTreeMap<String, BTreeSet<String>>,
    namespace_literals: BTreeMap<String, BTreeSet<String>>,
}

fn collect_source_usage(source_root: &Path) -> SourceUsage {
    let mut usage = SourceUsage::default();
    let mut stack = vec![source_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name != "target" && name != ".git" {
                    stack.push(path);
                }
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let where_ = path.display().to_string();
            for (literal, quote_offset) in lex_strings(&text) {
                // Template prefixes must be cut before the key-like guard: a
                // formatted family like "...validation.{reason}" is not itself
                // key-shaped, but its prefix is.
                if let Some(prefix) = template_prefix(&literal)
                    && !IGNORED_KEYS.contains(&prefix.as_str())
                {
                    usage
                        .template_prefixes
                        .entry(prefix)
                        .or_default()
                        .insert(where_.clone());
                }
                if !is_key_like(&literal) || IGNORED_KEYS.contains(&literal.as_str()) {
                    continue;
                }
                if is_direct_t_call(&text, quote_offset) {
                    usage
                        .direct
                        .entry(literal.clone())
                        .or_default()
                        .insert(where_.clone());
                }
                if literal.matches('.').count() >= 2 {
                    usage
                        .namespace_literals
                        .entry(literal)
                        .or_default()
                        .insert(where_.clone());
                }
            }
        }
    }
    usage
}

/// Extracts every string literal with its opening-quote byte offset, skipping
/// line comments, nested block comments, and character literals; raw strings
/// are collected with their bodies.
fn lex_strings(text: &str) -> Vec<(String, usize)> {
    let bytes = text.as_bytes();
    let mut literals = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                let mut depth = 1usize;
                i += 2;
                while i < bytes.len() && depth > 0 {
                    if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                        depth += 1;
                        i += 2;
                    } else if bytes[i] == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                        depth -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
            }
            b'r' if i + 1 < bytes.len() && (bytes[i + 1] == b'"' || bytes[i + 1] == b'#') => {
                let at_boundary =
                    i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
                if !at_boundary {
                    i += 1;
                    continue;
                }
                let mut hashes = 0usize;
                let mut j = i + 1;
                while j < bytes.len() && bytes[j] == b'#' {
                    hashes += 1;
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'"' {
                    let mut closer = vec![b'"'];
                    closer.extend(std::iter::repeat_n(b'#', hashes));
                    let mut end = j + 1;
                    while end + closer.len() <= bytes.len()
                        && &bytes[end..end + closer.len()] != closer.as_slice()
                    {
                        end += 1;
                    }
                    if end + closer.len() <= bytes.len() {
                        literals.push((text[j + 1..end].to_string(), i));
                        i = end + closer.len();
                    } else {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            b'"' => {
                let start = i + 1;
                let mut j = i + 1;
                while j < bytes.len() {
                    if bytes[j] == b'\\' {
                        j += 2;
                        continue;
                    }
                    if bytes[j] == b'"' {
                        break;
                    }
                    j += 1;
                }
                literals.push((text[start..j.min(bytes.len())].to_string(), i));
                i = j + 1;
            }
            b'\'' => {
                // Only a well-formed char literal ('x' or '\x') may be skipped
                // as a unit; lifetimes like 'static share the quote and must
                // not swallow the following code (which can contain string
                // literals).
                if i + 2 < bytes.len() && bytes[i + 2] == b'\'' {
                    i += 3;
                } else if i + 2 < bytes.len() && bytes[i + 1] == b'\\' {
                    let mut j = i + 2;
                    while j < bytes.len() {
                        if bytes[j] == b'\\' {
                            j += 2;
                            continue;
                        }
                        if bytes[j] == b'\'' {
                            break;
                        }
                        j += 1;
                    }
                    i = j + 1;
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    literals
}

fn is_key_like(literal: &str) -> bool {
    let segments: Vec<&str> = literal.split('.').collect();
    if segments.len() < 2 {
        return false;
    }
    segments.iter().all(|segment| {
        !segment.is_empty()
            && segment
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    }) && segments[0]
        .starts_with(|first: char| first.is_ascii_lowercase())
}

fn is_direct_t_call(text: &str, quote_offset: usize) -> bool {
    let start = quote_offset.saturating_sub(48);
    let tail = &text.as_bytes()[start..quote_offset];
    if tail.ends_with(b"i18n_with(") {
        return true;
    }
    if tail.ends_with(b"t(") {
        let before = tail.len() - 2;
        return before == 0
            || !(tail[before - 1].is_ascii_alphanumeric() || tail[before - 1] == b'_');
    }
    false
}

fn template_prefix(literal: &str) -> Option<String> {
    let brace = literal.find('{')?;
    if brace == 0 {
        return None;
    }
    let prefix = literal[..brace].trim_end_matches('.');
    is_key_like(prefix).then(|| prefix.to_string())
}
