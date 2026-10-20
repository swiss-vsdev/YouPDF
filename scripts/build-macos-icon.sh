#!/bin/sh
# Regénère l'icône macOS depuis docs/youPDF-icon.svg.
#
# macOS attend un artwork avec une marge transparente de 6,25 % (comme les
# icônes Apple) : sans elle, l'icône paraît trop grande dans le Dock. Le SVG
# source est réduit à 87,5 % et centré sur un canevas 512 transparent.
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
SVG="$ROOT/docs/youPDF-icon.svg"
ASSETS="$ROOT/crates/youpdf-app/src/assets"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
ICONSET="$TMP/YouPDF.iconset"

rsvg-convert -w 512 -h 512 "$SVG" -o "$TMP/raw.png"
magick "$TMP/raw.png" -resize 87.5% -background none -gravity center \
    -extent 512x512 "$ASSETS/YouPDF-icon.png"

mkdir -p "$ICONSET"
# Noms 1x/2x attendus par iconutil : taille, puis taille × 2.
for name in 16x16 32x32 128x128 256x256 512x512; do
    size=${name%x*}
    sips -z "$size" "$size" "$ASSETS/YouPDF-icon.png" \
        --out "$ICONSET/icon_$name.png" >/dev/null
    sips -z "$((size * 2))" "$((size * 2))" "$ASSETS/YouPDF-icon.png" \
        --out "$ICONSET/icon_${name}@2x.png" >/dev/null
done

# PNG 256 pour eframe (icône de fenêtre / Dock en `cargo run`).
sips -z 256 256 "$ASSETS/YouPDF-icon.png" --out "$ASSETS/icon.png" >/dev/null

iconutil -c icns -ov "$ICONSET" -o "$ASSETS/YouPDF.icns"
echo "Icône régénérée : $ASSETS/YouPDF.icns"
