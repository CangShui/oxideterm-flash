#!/usr/bin/env python3
"""Audit native locale catalogs against each other and against Rust source.

The audit enforces five independent invariants:

1. Exactly the required locales exist (a deleted locale pack is an error,
   not something the audit silently stops checking).
2. Every locale defines the same key set.
3. Every key referenced through a direct ``t("...")`` / ``i18n_with("...")``
   call exists in every locale.
4. Every dynamically formatted key family (``format!("...prefix_{var}")``)
   still has at least one catalog key, so a family cannot be deleted while
   its formatting code survives.
5. Every key-shaped string literal inside the i18n namespace (e.g. keys
   returned from ``label_key()`` helpers rather than passed inline to ``t``)
   exists in the catalogs.

Rules 4 and 5 are what stop "delete the same key from every locale at once"
from passing a languages-only comparison.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


DEFAULT_LOCALE_ROOT = Path("crates/oxideterm-i18n/locales")
DEFAULT_SOURCE_ROOTS = (Path("crates"),)
REQUIRED_LOCALES = ("en", "zh-CN")
PLACEHOLDER_RE = re.compile(r"\{\{\s*([A-Za-z0-9_.-]+)\s*\}\}")
KEY_SEGMENT_RE = re.compile(r"[A-Za-z0-9_]+\Z")
DIRECT_T_TAIL_RE = re.compile(r"(?:^|[^\w])(?:i18n_with|t)\($")
DEFAULT_IGNORED_SOURCE_KEYS = frozenset(
    {
        # This sentinel is intentionally used by oxideterm-i18n fallback tests.
        "missing.key",
    }
)


@dataclass
class LocaleCatalog:
    locale: str
    files: dict[str, Path] = field(default_factory=dict)
    values: dict[str, str] = field(default_factory=dict)
    placeholders: dict[str, set[str]] = field(default_factory=dict)
    duplicates: dict[str, list[str]] = field(default_factory=lambda: defaultdict(list))


@dataclass
class SourceKeyUsage:
    """Key references discovered in Rust source.

    direct: literals passed straight to ``t(...)`` / ``i18n_with(...)``.
    template_prefixes: literal prefixes of formatted key families, cut at the
        first ``{placeholder}``.
    namespace_literals: key-shaped literals with at least three segments; the
        audit keeps only those whose first two segments match an existing
        catalog family, which catches keys returned from ``label_key()``
        helpers instead of being passed inline to ``t``.
    """

    direct: dict[str, set[str]] = field(default_factory=lambda: defaultdict(set))
    template_prefixes: dict[str, set[str]] = field(default_factory=lambda: defaultdict(set))
    namespace_literals: dict[str, set[str]] = field(default_factory=lambda: defaultdict(set))


@dataclass
class AuditResult:
    catalogs: dict[str, LocaleCatalog]
    parse_errors: list[str]
    locale_set_mismatch: list[str]
    source_keys: dict[str, list[str]]
    missing_files: dict[str, list[str]]
    missing_by_locale: dict[str, list[str]]
    source_absent_everywhere: list[str]
    source_missing_by_locale: dict[str, list[str]]
    template_family_gaps: list[str]
    namespace_missing: list[str]
    placeholder_mismatches: dict[str, dict[str, list[str]]]
    english_copies: dict[str, list[str]]

    def has_errors(self) -> bool:
        return bool(
            self.parse_errors
            or self.locale_set_mismatch
            or any(catalog.duplicates for catalog in self.catalogs.values())
            or self.missing_files
            or self.missing_by_locale
            or self.source_absent_everywhere
            or self.source_missing_by_locale
            or self.template_family_gaps
            or self.namespace_missing
            or self.placeholder_mismatches
        )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Check native i18n JSON catalogs for missing keys, duplicate flattened "
            "keys, placeholder drift, locale-set drift, and source-referenced keys "
            "that no locale defines (including dynamically formatted families)."
        )
    )
    parser.add_argument(
        "--locale-root",
        type=Path,
        default=DEFAULT_LOCALE_ROOT,
        help="Directory containing locale subdirectories.",
    )
    parser.add_argument(
        "--source-root",
        type=Path,
        action="append",
        default=None,
        help="Source root to scan for i18n key usage. May be passed multiple times.",
    )
    parser.add_argument(
        "--fail-on-english-copy",
        action="store_true",
        help="Treat non-English values identical to English as errors.",
    )
    parser.add_argument(
        "--ignore-source-key",
        action="append",
        default=[],
        help="Static source key to ignore. Intended for explicit fallback-test sentinels.",
    )
    parser.add_argument(
        "--show-all",
        action="store_true",
        help="Print every finding instead of truncating long sections.",
    )
    return parser.parse_args()


def flatten_json(value: Any, prefix: str, out: dict[str, str]) -> None:
    if isinstance(value, dict):
        for key, child in value.items():
            next_prefix = f"{prefix}.{key}" if prefix else str(key)
            flatten_json(child, next_prefix, out)
        return
    if isinstance(value, str):
        out[prefix] = value


def load_catalogs(locale_root: Path) -> tuple[dict[str, LocaleCatalog], list[str]]:
    catalogs: dict[str, LocaleCatalog] = {}
    parse_errors: list[str] = []
    for locale_dir in sorted(path for path in locale_root.iterdir() if path.is_dir()):
        catalog = LocaleCatalog(locale=locale_dir.name)
        for json_file in sorted(locale_dir.glob("*.json")):
            catalog.files[json_file.name] = json_file
            try:
                data = json.loads(json_file.read_text(encoding="utf-8"))
            except Exception as error:  # noqa: BLE001 - report exact parser failure.
                parse_errors.append(f"{json_file}: {error}")
                continue
            flattened: dict[str, str] = {}
            flatten_json(data, "", flattened)
            for key, text in flattened.items():
                if key in catalog.values:
                    catalog.duplicates[key].append(str(json_file))
                    continue
                catalog.values[key] = text
                catalog.placeholders[key] = set(PLACEHOLDER_RE.findall(text))
        catalogs[catalog.locale] = catalog
    return catalogs, parse_errors


def lex_rust_strings(text: str) -> list[tuple[str, int, int]]:
    """Return (literal, start, end) for every Rust string literal.

    The lexer skips line comments, nested block comments, and character
    literals so documentation never surfaces as key references. Raw strings
    (``r"..."`` and ``r#"..."#``) are collected with their bodies.
    """
    literals: list[tuple[str, int, int]] = []
    i = 0
    n = len(text)
    while i < n:
        ch = text[i]
        if ch == "/" and i + 1 < n and text[i + 1] == "/":
            newline = text.find("\n", i)
            i = n if newline == -1 else newline + 1
        elif ch == "/" and i + 1 < n and text[i + 1] == "*":
            depth = 1
            i += 2
            while i < n and depth:
                if text.startswith("/*", i):
                    depth += 1
                    i += 2
                elif text.startswith("*/", i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
        elif ch == "r" and i + 1 < n and (text[i + 1] == '"' or text[i + 1] == "#"):
            hashes = 0
            j = i + 1
            while j < n and text[j] == "#":
                hashes += 1
                j += 1
            if j < n and text[j] == '"':
                closer = '"' + "#" * hashes
                end = text.find(closer, j + 1)
                if end == -1:
                    break
                literals.append((text[j + 1 : end], i, end))
                i = end + len(closer)
            else:
                i += 1
        elif ch == '"':
            j = i + 1
            body_start = j
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == '"':
                    break
                j += 1
            literals.append((text[body_start:j], i, j + 1))
            i = j + 1
        elif ch == "'":
            # Only a well-formed char literal ('x' or '\x') may be skipped as
            # a unit; lifetimes like 'static share the quote and must not
            # swallow the following code (which can contain string literals).
            if i + 2 < n and text[i + 2] == "'":
                i += 3
            elif i + 2 < n and text[i + 1] == "\\":
                j = i + 2
                while j < n:
                    if text[j] == "\\":
                        j += 2
                        continue
                    if text[j] == "'":
                        break
                    j += 1
                i = j + 1
            else:
                i += 1
        else:
            i += 1
    return literals


def is_key_like(literal: str) -> bool:
    if not literal or "." not in literal:
        return False
    segments = literal.split(".")
    if not all(KEY_SEGMENT_RE.fullmatch(segment) for segment in segments):
        return False
    first = segments[0][:1]
    return first.isascii() and first.islower()


def is_direct_t_call(text: str, quote_offset: int) -> bool:
    tail = text[max(0, quote_offset - 48) : quote_offset]
    return bool(DIRECT_T_TAIL_RE.search(tail))


def is_action_id_occurrence(text: str, start: int, end: int) -> bool:
    """Match arms and equality comparisons carry action ids, not i18n keys.

    Keybinding tables dispatch on string literals ("terminal.paste" => ...),
    and those ids share the dot-separated shape of translation keys.
    """
    after = text[end : end + 8].lstrip()
    if after.startswith("=>"):
        return True
    before = text[max(0, start - 3) : start].rstrip()
    return before.endswith("==")


def template_prefix(literal: str) -> str | None:
    brace = literal.find("{")
    if brace <= 0:
        return None
    prefix = literal[:brace]
    # A placeholder directly after a dot ("...validation.{reason}") leaves a
    # trailing dot; trim it so the family prefix still resolves to keys.
    prefix = prefix.rstrip(".")
    if not prefix:
        return None
    # Single-segment prefixes ("onboarding.{fragment}") compose a whole
    # namespace at runtime; they must still resolve against catalog roots.
    if "." in prefix:
        return prefix if is_key_like(prefix) else None
    return prefix if KEY_SEGMENT_RE.fullmatch(prefix) else None


def collect_source_key_usage(
    source_roots: tuple[Path, ...], ignored_source_keys: set[str]
) -> SourceKeyUsage:
    usage = SourceKeyUsage()
    for root in source_roots:
        if not root.exists():
            continue
        for source_file in root.rglob("*.rs"):
            if any(part in {"target", ".git"} for part in source_file.parts):
                continue
            try:
                text = source_file.read_text(encoding="utf-8")
            except UnicodeDecodeError:
                continue
            where = str(source_file)
            for literal, quote_start, quote_end in lex_rust_strings(text):
                # Template prefixes must be cut from the raw literal before the
                # key-like guard: a formatted family like
                # "settings_view.terminal.highlight_rules.validation.{reason}"
                # is not itself key-shaped, but its prefix is.
                prefix = template_prefix(literal)
                if prefix is not None and prefix not in ignored_source_keys:
                    usage.template_prefixes[prefix].add(where)
                if not is_key_like(literal) or literal in ignored_source_keys:
                    continue
                if is_direct_t_call(text, quote_start):
                    usage.direct[literal].add(where)
                action_shaped = is_action_id_occurrence(text, quote_start, quote_end)
                if len(literal.split(".")) >= 2 and not action_shaped:
                    usage.namespace_literals[literal].add(where)
    return usage


def audit(
    locale_root: Path, source_roots: tuple[Path, ...], ignored_source_keys: set[str]
) -> AuditResult:
    catalogs, parse_errors = load_catalogs(locale_root)
    usage = collect_source_key_usage(source_roots, ignored_source_keys)

    locale_set_mismatch: list[str] = []
    expected_locales = set(REQUIRED_LOCALES)
    found_locales = set(catalogs)
    for locale in sorted(expected_locales - found_locales):
        locale_set_mismatch.append(f"required locale missing: {locale}")
    for locale in sorted(found_locales - expected_locales):
        locale_set_mismatch.append(f"unexpected locale directory: {locale}")

    file_union = sorted({file_name for catalog in catalogs.values() for file_name in catalog.files})
    missing_files = {
        locale: [file_name for file_name in file_union if file_name not in catalog.files]
        for locale, catalog in catalogs.items()
    }
    missing_files = {locale: files for locale, files in missing_files.items() if files}

    key_union = sorted({key for catalog in catalogs.values() for key in catalog.values})
    missing_by_locale = {
        locale: [key for key in key_union if key not in catalog.values]
        for locale, catalog in catalogs.items()
    }
    missing_by_locale = {locale: keys for locale, keys in missing_by_locale.items() if keys}

    any_locale_keys = set(key_union)
    source_absent_everywhere = sorted(key for key in usage.direct if key not in any_locale_keys)
    source_missing_by_locale = {
        locale: sorted(
            key
            for key in usage.direct
            if key in any_locale_keys and key not in catalog.values
        )
        for locale, catalog in catalogs.items()
    }
    source_missing_by_locale = {
        locale: keys for locale, keys in source_missing_by_locale.items() if keys
    }

    # Only enforce families inside the i18n namespace: a ``format!`` template
    # whose root segment matches no catalog root (keychain service names,
    # config paths) is not a translation family.
    catalog_roots = {key.split(".", 1)[0] for key in key_union}
    def family_matches(prefix: str) -> bool:
        needle = f"{prefix}." if "." not in prefix else prefix
        return any(key.startswith(needle) for key in any_locale_keys)

    template_family_gaps = sorted(
        f"{prefix}* <- {', '.join(sorted(paths)[:3])}"
        for prefix, paths in usage.template_prefixes.items()
        if prefix.split(".", 1)[0] in catalog_roots
        and not family_matches(prefix)
    )

    catalog_families = {
        ".".join(key.split(".")[:2]) for key in key_union if key.count(".") >= 1
    }
    namespace_missing = sorted(
        f"{literal} <- {', '.join(sorted(paths)[:3])}"
        for literal, paths in usage.namespace_literals.items()
        if ".".join(literal.split(".")[:2]) in catalog_families
        and literal not in any_locale_keys
    )
    # Namespace literals participate in per-locale enforcement exactly like
    # direct keys: deleting one locale's copy of a composed key is a drift.
    for literal in usage.namespace_literals:
        if ".".join(literal.split(".")[:2]) not in catalog_families:
            continue
        for locale, catalog in catalogs.items():
            if literal in any_locale_keys and literal not in catalog.values:
                source_missing_by_locale.setdefault(locale, []).append(literal)
    source_missing_by_locale = {
        locale: sorted(set(keys)) for locale, keys in source_missing_by_locale.items() if keys
    }

    placeholder_mismatches: dict[str, dict[str, list[str]]] = {}
    for key in key_union:
        expected_sets = {
            locale: catalog.placeholders.get(key, set())
            for locale, catalog in catalogs.items()
            if key in catalog.values
        }
        unique_sets = {tuple(sorted(placeholders)) for placeholders in expected_sets.values()}
        if len(unique_sets) <= 1:
            continue
        placeholder_mismatches[key] = {
            locale: sorted(placeholders) for locale, placeholders in expected_sets.items()
        }

    english_copies: dict[str, list[str]] = {}
    english = catalogs.get("en")
    if english:
        for locale, catalog in catalogs.items():
            if locale == "en":
                continue
            copied = []
            for key, english_text in english.values.items():
                local_text = catalog.values.get(key)
                if not local_text or local_text != english_text:
                    continue
                # Keep empty strings and pure tokens out of the signal.
                if not english_text.strip() or not re.search(r"[A-Za-z]{4,}", english_text):
                    continue
                copied.append(key)
            if copied:
                english_copies[locale] = copied

    return AuditResult(
        catalogs=catalogs,
        parse_errors=parse_errors,
        locale_set_mismatch=locale_set_mismatch,
        source_keys={key: sorted(paths) for key, paths in sorted(usage.direct.items())},
        missing_files=missing_files,
        missing_by_locale=missing_by_locale,
        source_absent_everywhere=source_absent_everywhere,
        source_missing_by_locale=source_missing_by_locale,
        template_family_gaps=template_family_gaps,
        namespace_missing=namespace_missing,
        placeholder_mismatches=placeholder_mismatches,
        english_copies=english_copies,
    )


def print_limited(title: str, items: list[str], limit: int | None = 30) -> None:
    print(f"\n{title}: {len(items)}")
    visible_items = items if limit is None else items[:limit]
    for item in visible_items:
        print(f"  - {item}")
    if limit is not None and len(items) > limit:
        print(f"  ... {len(items) - limit} more")


def print_result(result: AuditResult, fail_on_english_copy: bool, show_all: bool) -> int:
    limit = None if show_all else 30
    print(f"Locales: {', '.join(sorted(result.catalogs))}")
    print(f"Source i18n keys scanned: {len(result.source_keys)}")

    print_limited("JSON parse errors", result.parse_errors, limit)
    print_limited("Locale set mismatches", result.locale_set_mismatch, limit)

    duplicate_lines = []
    for locale, catalog in sorted(result.catalogs.items()):
        for key, files in sorted(catalog.duplicates.items()):
            duplicate_lines.append(f"{locale}:{key} ({', '.join(files)})")
    print_limited("Duplicate flattened keys", duplicate_lines, limit)

    missing_file_lines = [
        f"{locale}: {', '.join(files)}" for locale, files in sorted(result.missing_files.items())
    ]
    print_limited("Missing locale files", missing_file_lines, limit)

    missing_key_lines = [
        f"{locale}: {key}"
        for locale, keys in sorted(result.missing_by_locale.items())
        for key in keys
    ]
    print_limited("Keys missing from locale catalogs", missing_key_lines, limit)

    absent_lines = [
        f"{key} <- {', '.join(paths[:3])}"
        for key, paths in result.source_keys.items()
        if key in result.source_absent_everywhere
    ]
    print_limited("Source-used keys absent from every locale", absent_lines, limit)

    source_missing_lines = [
        f"{locale}: {key}"
        for locale, keys in sorted(result.source_missing_by_locale.items())
        for key in keys
    ]
    print_limited("Source-used keys missing from some locales", source_missing_lines, limit)

    print_limited("Formatted key families with no keys", result.template_family_gaps, limit)
    print_limited("Namespace literals missing from catalogs", result.namespace_missing, limit)

    placeholder_lines = [
        f"{key}: {per_locale}"
        for key, per_locale in sorted(result.placeholder_mismatches.items())
    ]
    print_limited("Placeholder mismatches", placeholder_lines, limit)

    english_copy_lines = [
        f"{locale}: {key}"
        for locale, keys in sorted(result.english_copies.items())
        for key in keys
    ]
    heading = "English-copy warnings"
    if fail_on_english_copy:
        heading = "English-copy errors"
    print_limited(heading, english_copy_lines, limit)

    has_structural_errors = result.has_errors()
    has_english_copy_errors = fail_on_english_copy and bool(result.english_copies)
    if has_structural_errors or has_english_copy_errors:
        return 1
    return 0


def main() -> int:
    args = parse_args()
    source_roots = tuple(args.source_root) if args.source_root else DEFAULT_SOURCE_ROOTS
    ignored_source_keys = set(DEFAULT_IGNORED_SOURCE_KEYS)
    ignored_source_keys.update(args.ignore_source_key)
    result = audit(args.locale_root, source_roots, ignored_source_keys)
    return print_result(result, args.fail_on_english_copy, args.show_all)


if __name__ == "__main__":
    sys.exit(main())
