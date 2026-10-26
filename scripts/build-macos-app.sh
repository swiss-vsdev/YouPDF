#!/bin/sh
# Assemble un .app macOS avec l'icône native (YouPDF.icns).
#
# Le format .icns est le format d'icône demandé par macOS : il est posé dans
# Contents/Resources et déclaré par CFBundleIconFile dans Info.plist. Pour un
# simple `cargo run`, l'icône est fournie à eframe en PNG (assets/icon.png).
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
APP="$ROOT/target/YouPDF.app"
VERSION=$(sed -n 's/^version[[:space:]]*=[[:space:]]*"\(.*\)".*/\1/p' "$ROOT/Cargo.toml" | head -1)

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cp "$ROOT/crates/youpdf-app/src/assets/YouPDF.icns" "$APP/Contents/Resources/YouPDF.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key><string>YouPDF</string>
	<key>CFBundleDisplayName</key><string>YouPDF</string>
	<key>CFBundleIdentifier</key><string>com.youpdf.app</string>
	<key>CFBundleVersion</key><string>$VERSION</string>
	<key>CFBundleShortVersionString</key><string>$VERSION</string>
	<key>CFBundleIconFile</key><string>YouPDF.icns</string>
	<key>CFBundleExecutable</key><string>YouPDF</string>
	<key>CFBundlePackageType</key><string>APPL</string>
	<key>LSMinimumSystemVersion</key><string>11.0</string>
	<key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

BIN="$ROOT/target/release/youpdf"
if [ ! -f "$BIN" ]; then
	cargo build --release -p youpdf-app
fi
cp "$BIN" "$APP/Contents/MacOS/YouPDF"

# PDFium est cherché à côté de l'exécutable : on le copie dans MacOS/.
if [ -f "$ROOT/pdfium/libpdfium.dylib" ]; then
	cp "$ROOT/pdfium/libpdfium.dylib" "$APP/Contents/MacOS/"
	xattr -d com.apple.quarantine "$APP/Contents/MacOS/libpdfium.dylib" 2>/dev/null || true
fi

# Tesseract optionnel : placé dans Contents/MacOS/bin/ (recherché avant PATH).
if command -v tesseract >/dev/null 2>&1; then
	mkdir -p "$APP/Contents/MacOS/bin"
	cp "$(which tesseract)" "$APP/Contents/MacOS/bin/"
fi

# Empaquetage : .app zippé.
ARCH="${1:-$(uname -m)}"
APP_NAME="YouPDF-macos-$ARCH-$VERSION.app"
ZIP_PATH="$ROOT/target/release/$APP_NAME.zip"
rm -f "$ZIP_PATH"
cd "$ROOT/target"
zip -rq "$ZIP_PATH" YouPDF.app
echo "Bundle : $APP"
echo "Zip   : $ZIP_PATH"
