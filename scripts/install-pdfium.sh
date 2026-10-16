#!/usr/bin/env bash
# Télécharge (ou répare) la bibliothèque pdfium dans ./pdfium/.
#
# Usage : ./scripts/install-pdfium.sh [version]
#         ex : ./scripts/install-pdfium.sh chromium/6729
#
# Sur macOS l'étape de réparation est indispensable : une dylib téléchargée
# porte l'attribut com.apple.quarantine, et dyld refuse alors de la charger
# ("library load disallowed by system policy"), ce qui fait retomber YouPDF
# sur NullRenderer (pages grises).
set -euo pipefail

VERSION="${1:-${PDFIUM_VERSION:-chromium/6729}}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT/pdfium"

os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
  Darwin)
    case "$arch" in
      arm64)  ARTIFACT="pdfium-mac-arm64.zip"  LIB="libpdfium.dylib" ;;
      x86_64) ARTIFACT="pdfium-mac-x64.zip"    LIB="libpdfium.dylib" ;;
      *) echo "architecture macOS non supportée : $arch" >&2; exit 1 ;;
    esac
    ;;
  Linux)
    case "$arch" in
      x86_64)  ARTIFACT="pdfium-linux-x64.tgz" LIB="libpdfium.so" ;;
      aarch64) ARTIFACT="pdfium-linux-arm64.tgz" LIB="libpdfium.so" ;;
      *) echo "architecture Linux non supportée : $arch" >&2; exit 1 ;;
    esac
    ;;
  *) echo "plateforme non supportée : $os (sous Windows, placez pdfium.dll dans pdfium\\)" >&2; exit 1 ;;
esac

mkdir -p "$DEST"

if [ ! -f "$DEST/$LIB" ]; then
  url="https://github.com/bblanchon/pdfium-binaries/releases/download/$VERSION/$ARTIFACT"
  echo "→ téléchargement $url"
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  curl -fL --retry 3 -o "$tmp/$ARTIFACT" "$url"
  case "$ARTIFACT" in
    *.zip) unzip -q -o "$tmp/$ARTIFACT" -d "$tmp" ;;
    *.tgz) tar xzf "$tmp/$ARTIFACT" -C "$tmp" ;;
  esac
  src="$(find "$tmp" -name "$LIB" -type f | head -n1)"
  [ -n "$src" ] || { echo "bibliothèque $LIB absente de l'archive" >&2; exit 1; }
  cp "$src" "$DEST/$LIB"
  chmod 644 "$DEST/$LIB"
fi

# Réparation macOS : quarantine + signature ad-hoc (obligatoire sur arm64,
# la signature du binaire pdfium amont est invalide une fois ré-empaquetée).
if [ "$os" = "Darwin" ]; then
  xattr -d com.apple.quarantine "$DEST/$LIB" 2>/dev/null || true
  codesign --force --sign - "$DEST/$LIB"
  codesign --verify --verbose=1 "$DEST/$LIB"
fi

echo "✓ $(ls -lh "$DEST/$LIB" | awk '{print $5, $9}')"
