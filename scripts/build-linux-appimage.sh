#!/usr/bin/env bash
# Assemble un paquet Linux autocontenu (tar.gz + script d'exécution).
#
# Le binaire YouPDF a besoin de libpdfium.so à côté de lui ; on le place
# dans ./usr/lib/ à l'intérieur de l'archive, et on utilise un script
# d'entrée qui le copie à côté du binaire avant de lancer.
#
# Usage : ./scripts/build-linux-appimage.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RELEASE="$ROOT/target/release"
OUT="$ROOT/target/release/YouPDF-linux-x64"
VERSION=$(sed -n 's/^version[[:space:]]*=[[:space:]]*"\(.*\)".*/\1/p' "$ROOT/Cargo.toml" | head -1)

rm -rf "$OUT"
mkdir -p "$OUT/usr/bin" "$OUT/usr/lib"

# Binaire principal
cp "$RELEASE/youpdf" "$OUT/usr/bin/youpdf"
chmod +x "$OUT/usr/bin/youpdf"

# pdfium
if [ -f "$ROOT/pdfium/libpdfium.so" ]; then
    cp "$ROOT/pdfium/libpdfium.so" "$OUT/usr/lib/"
    # RPATH relatif : le binaire cherche libpdfium.so dans le répertoire
    # de l'executable (./usr/lib/ est un ancêtre).
    patchelf --set-rpath '$ORIGIN/../lib' "$OUT/usr/bin/youpdf" 2>/dev/null || true
fi

# Script d'entrée
cat > "$OUT/youpdf" <<'EOF'
#!/usr/bin/env bash
DIR="$(cd "$(dirname "$0")" && pwd)"
export LD_LIBRARY_PATH="$DIR/usr/lib:${LD_LIBRARY_PATH:-}"
exec "$DIR/usr/bin/youpdf" "$@"
EOF
chmod +x "$OUT/youpdf"

# tesseract optionnel : placé dans ./usr/bin/bin/ à côté du binaire réel
if command -v tesseract >/dev/null 2>&1; then
    mkdir -p "$OUT/usr/bin/bin"
    cp "$(which tesseract)" "$OUT/usr/bin/bin/"
fi

# Archive
cd "$ROOT/target/release"
tar czf "YouPDF-linux-x64-$VERSION.tar.gz" -C "$(dirname "$OUT")" "$(basename "$OUT")"
echo "✓ $(ls -lh "YouPDF-linux-x64-$VERSION.tar.gz" | awk '{print $5, $9}')"