#!/usr/bin/env bash
# Assemble un paquet Windows portable (zip) avec pdfium.dll et tesseract.
#
# Usage : ./scripts/build-windows-zip.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RELEASE="$ROOT/target/release"
OUT="$ROOT/target/release/YouPDF-win64"
VERSION=$(sed -n 's/^version[[:space:]]*=[[:space:]]*"\(.*\)".*/\1/p' "$ROOT/Cargo.toml" | head -1)

rm -rf "$OUT"
mkdir -p "$OUT/bin"

# Binaire principal
cp "$RELEASE/youpdf.exe" "$OUT/"

# pdfium
if [ -f "$ROOT/pdfium/pdfium.dll" ]; then
    cp "$ROOT/pdfium/pdfium.dll" "$OUT/"
fi

# Tesseract optionnel
if command -v tesseract >/dev/null 2>&1; then
    cp "$(which tesseract)" "$OUT/bin/"
fi

# Archive
cd "$ROOT/target/release"
zip -rq "YouPDF-win64-$VERSION.zip" YouPDF-win64
echo "✓ $(ls -lh "YouPDF-win64-$VERSION.zip" | awk '{print $5, $9}')"