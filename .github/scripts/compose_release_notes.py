#!/usr/bin/env python3
"""Compose release notes from channel-specific evergreen text and one changelog entry."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path
from urllib.parse import quote


CHANGELOG_MARKER = "<!-- RELEASE_CHANGELOG -->"
DOWNLOADS_MARKER = "<!-- RELEASE_DOWNLOADS -->"
REPOSITORY_RELEASE_URL = "https://github.com/CangShui/oxideterm-flash/releases/download"


def normalize_leading_summary(section: str) -> str:
    """Join soft-wrapped lines in the opening prose paragraph."""
    lines = section.splitlines()
    if not lines:
        return section

    paragraph_end = next(
        (index for index, line in enumerate(lines) if not line.strip()),
        len(lines),
    )
    paragraph = lines[:paragraph_end]
    if not paragraph:
        return section

    # Preserve Markdown blocks when a channel intentionally starts with one.
    if any(
        re.match(r"^(?:#{1,6}\s|[-*+]\s|>\s?|```|~~~|\d+[.)]\s|\||<)", line.lstrip())
        for line in paragraph
    ):
        return section

    summary = " ".join(line.strip() for line in paragraph)
    remainder = lines[paragraph_end:]
    return "\n".join([summary, *remainder])


def release_asset_url(tag: str, filename: str) -> str:
    """Build a URL for an asset attached to the release being composed."""
    return f"{REPOSITORY_RELEASE_URL}/{quote(tag, safe='')}/{quote(filename, safe='')}"


def stable_download_table(version: str, tag: str) -> str:
    """Render this fork's Windows-only download table from its real release assets."""
    filenames = {
        "app": "oxideterm-native.exe",
        "cli": "oxideterm.exe",
        "rdp": "oxideterm-rdp-helper.exe",
        "vnc": "oxideterm-vnc-helper.exe",
        "sums": "SHA256SUMS.txt",
    }

    def link(label: str, key: str) -> str:
        return f"[{label}]({release_asset_url(tag, filenames[key])})"

    return "\n".join(
        [
            "## \U0001f4e5 Download (Windows x64)",
            "",
            "| Artifact | Link |",
            "|---|---|",
            f"| **OxideTerm** (GUI) | {link('oxideterm-native.exe', 'app')} |",
            f"| **OxideTerm CLI** | {link('oxideterm.exe', 'cli')} |",
            f"| **RDP helper** | {link('oxideterm-rdp-helper.exe', 'rdp')} |",
            f"| **VNC helper** | {link('oxideterm-vnc-helper.exe', 'vnc')} |",
            f"| **SHA256 checksums** | {link('SHA256SUMS.txt', 'sums')} |",
            "",
            "This community fork currently ships Windows x64 builds only.",
        ]
    )


def extract_version_section(
    changelog: str, version: str, *, include_heading: bool = True
) -> str:
    """Return one version entry, optionally retaining its extraction heading."""
    pattern = re.compile(
        rf"^## {re.escape(version)}\n(?P<section>.*?)(?=^## |\Z)",
        re.MULTILINE | re.DOTALL,
    )
    match = pattern.search(changelog)
    if match is None:
        raise ValueError(f"no changelog section found for {version}")
    section = normalize_leading_summary(match.group("section").strip())
    if include_heading:
        return f"## {version}\n\n{section}"
    return section


def compose_notes(version: str, tag: str, base_path: Path, changelog_path: Path) -> str:
    base = base_path.read_text(encoding="utf-8")
    if CHANGELOG_MARKER not in base:
        raise ValueError(f"{base_path} is missing {CHANGELOG_MARKER}")

    changelog = changelog_path.read_text(encoding="utf-8")
    # Stable releases already show the version in GitHub's release title, so their
    # body starts directly with the summary while other channels keep the heading.
    is_stable_release = DOWNLOADS_MARKER in base
    section = extract_version_section(
        changelog, version, include_heading=not is_stable_release
    )
    notes = base.replace(CHANGELOG_MARKER, section)
    if DOWNLOADS_MARKER in notes:
        notes = notes.replace(DOWNLOADS_MARKER, stable_download_table(version, tag))
    return notes.rstrip() + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--base", required=True, type=Path)
    parser.add_argument("--changelog", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    try:
        notes = compose_notes(args.version, args.tag, args.base, args.changelog)
    except Exception as error:
        print(f"failed to compose release notes: {error}", file=sys.stderr)
        return 1

    args.output.write_text(notes, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
