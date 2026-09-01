python := if os() == "windows" { "py -3" } else { "python3" }

# List the available repository tasks.
default:
    @just --list

# Check Rust formatting without rewriting files.
fmt:
    cargo fmt --all -- --check

# Verify that every user-facing string goes through the locale catalogs: any
# i18n key referenced from Rust but missing from all locale packs, any locale
# file/key/placeholder drift, any deleted locale pack, or any dynamically
# formatted family that lost all of its keys fails this step. The second
# command is the same gate as a Rust test, so plain `cargo test` enforces it.
quality-i18n:
    {{ python }} scripts/quality/audit_i18n.py --show-all
    cargo test --release -p oxideterm-i18n

# Run OxideTerm with optional Cargo arguments.
run *args:
    cargo run {{ args }}

# Generate the aggregated third-party license notices.
notices:
    {{ python }} scripts/release/generate_third_party_notices.py

# Build and stage the CLI companion for an optional target triple.
build-cli target="":
    bash scripts/build/build-cli.sh {{ target }}

# Run the terminal throughput benchmark in the active OxideTerm terminal.
benchmark:
    sh benchmark/benchmark.sh

# Update the workspace version, with optional bump script flags.
bump-version version *options:
    {{ python }} scripts/release/bump_version.py {{ options }} {{ version }}

# Build native release packages for an optional target triple.
package target="":
    {{ python }} scripts/release/package_native.py {{ target }}
