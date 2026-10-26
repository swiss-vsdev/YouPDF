#!/usr/bin/env pwsh
# Télécharge (ou répare) la bibliothèque pdfium dans ./pdfium/.
#
# Usage : .\scripts\install-pdfium.ps1 [version]
#         ex : .\scripts\install-pdfium.ps1 chromium/6729
#
# Sur Windows, placez pdfium.dll dans .\pdfium\ à la racine du dépôt.

$ErrorActionPreference = "Stop"
$VERSION = if ($args.Count -gt 0) { $args[0] } else { $env:PDFIUM_VERSION }
if (-not $VERSION) { $VERSION = "chromium/6729" }

$ROOT = Split-Path -Parent $PSScriptRoot
$DEST = Join-Path $ROOT "pdfium"
$LIB = "pdfium.dll"
$ARTIFACT = "pdfium-win-x86_64.zip"

if (-not (Test-Path $DEST)) {
    New-Item -ItemType Directory -Force -Path $DEST | Out-Null
}

if (-not (Test-Path (Join-Path $DEST $LIB))) {
    $url = "https://github.com/bblanchon/pdfium-binaries/releases/download/$VERSION/$ARTIFACT"
    Write-Host "→ Téléchargement $url"
    $tmp = Join-Path $env:TEMP "pdfium-$([Guid]::NewGuid())"
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    try {
        Invoke-WebRequest -Uri $url -OutFile (Join-Path $tmp $ARTIFACT) -UseBasicParsing
        Expand-Archive -Path (Join-Path $tmp $ARTIFACT) -DestinationPath $tmp -Force
        $src = Get-ChildItem -Path $tmp -Recurse -Filter $LIB | Select-Object -First 1
        if (-not $src) {
            Write-Error "bibliothèque $LIB absente de l'archive"
            exit 1
        }
        Copy-Item $src.FullName (Join-Path $DEST $LIB) -Force
        Write-Host "✓ $(Get-Item (Join-Path $DEST $LIB)).Length octets"
    } finally {
        Remove-Item -Recurse -Force $tmp
    }
} else {
    Write-Host "✓ pdfium.dll déjà présent"
}