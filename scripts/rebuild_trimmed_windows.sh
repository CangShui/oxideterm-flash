#!/bin/bash
# Rebuild trimmed Windows release binaries and analyze size composition.
# Usage: bash scripts/rebuild_trimmed_windows.sh
set -e
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR="$HOME/oxideterm-target"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_AR=x86_64-w64-mingw32-ar
export RUSTFLAGS="-C debug-assertions=on"
export CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_DEBUG_ASSERTIONS=true

echo "== check =="
cargo check --target x86_64-pc-windows-gnu -p oxideterm-gpui-app --message-format=short 2>&1 | grep -cE "error\[" || true

echo "== build native + cli =="
cargo build --release --target x86_64-pc-windows-gnu -p oxideterm-gpui-app -p oxideterm-cli

NATIVE=/root/oxideterm-target/x86_64-pc-windows-gnu/release/oxideterm-native.exe
CLI=/root/oxideterm-target/x86_64-pc-windows-gnu/release/oxideterm.exe

echo "== size =="
ls -la "$NATIVE" "$CLI"
x86_64-w64-mingw32-size "$NATIVE"

echo "== dependencies =="
x86_64-w64-mingw32-objdump -p "$NATIVE" | grep "DLL Name" | sort -u

echo "== static lib aggregate (one copy per crate) =="
find "$CARGO_TARGET_DIR/x86_64-pc-windows-gnu/release/build" -maxdepth 1 -mindepth 1 -type d | while read d; do
  f=$(find "$d/out" -maxdepth 1 -name "*.a" 2>/dev/null | head -1)
  if [ -n "$f" ]; then sz=$(stat -c %s "$f"); name=$(basename "$d" | sed 's/-[a-f0-9]*$//'); echo "$sz $name"; fi
done | sort -u -k2 | sort -rn > /tmp/trimmed_libs.txt
python3 - <<'PYEOF'
from collections import defaultdict
cats = defaultdict(int); total = 0
for line in open('/tmp/trimmed_libs.txt'):
    sz, name = line.split(None, 1); sz = int(sz); name = name.strip(); total += sz
    if name.startswith('tree-sitter') or name.startswith('ts-parser'):
        cats['tree-sitter grammars'] += sz
    else: cats[name] += sz
for k, v in sorted(cats.items(), key=lambda x: -x[1]):
    print(f'{v/1048576:7.2f} MB  {k}')
print(f'{total/1048576:7.2f} MB  TOTAL static libs')
PYEOF

echo "== deliver to workspace =="
mkdir -p dist/windows-x64
cp "$NATIVE" dist/windows-x64/oxideterm-native.exe
cp "$CLI" dist/windows-x64/oxideterm.exe
cd dist/windows-x64 && sha256sum *.exe > SHA256SUMS.txt && cat SHA256SUMS.txt
