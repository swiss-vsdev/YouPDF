# Bibliothèque pdfium

Ce dossier accueille la bibliothèque native `pdfium` (Chrome engine)
utilisée par YouPDF pour le rendu des pages PDF.

## Pourquoi pdfium ?

C'est la bibliothèque de rendu PDF utilisée par Chrome. Excellente
fidélité, licence BSD, aucune dépendance à Adobe.

## Installation rapide

```bash
./scripts/install-pdfium.sh          # détecte l'OS/l'arch, télécharge, signe
YOUPDF_PDFIUM_DIR=/opt/pdfium ./target/release/youpdf   # ou chemin personnalisé
```

## Installation manuelle

Récupérez le binaire correspondant à votre plateforme sur :

<https://github.com/bblanchon/pdfium-binaries>

Puis copiez le fichier dans ce dossier :

| Plateforme | Fichier à placer | Artifact |
|---|---|---|
| Linux x86_64  | `libpdfium.so`   | `pdfium-linux-x64.tgz` |
| Linux aarch64 | `libpdfium.so`   | `pdfium-linux-arm64.tgz` |
| macOS x86_64  | `libpdfium.dylib` | `pdfium-mac-x64.zip` |
| macOS aarch64 | `libpdfium.dylib` | `pdfium-mac-arm64.zip` |
| Windows x64   | `pdfium.dll`      | `pdfium-win-x64.tgz` |

> Les fichiers sont à la racine de `./pdfium/` (pas dans un sous-dossier
> `lib/`). L'ordre de recherche : `$YOUPDF_PDFIUM_DIR`, `$PDFIUM_DIR`,
> `./pdfium/` et `./` à côté de l'exécutable, puis en remontant depuis
> l'exécutable (`target/release/` → racine du dépôt), puis le répertoire
> courant, puis les chemins système.

## macOS : « library load disallowed by system policy »

Une `dylib` téléchargée porte l'attribut étendu `com.apple.quarantine` et sa
signature ad-hoc est souvent invalide une fois ré-empaquetée : `dlopen`
échoue même si le fichier est présent, et YouPDF retombe silencieusement sur
le rendu factice. Réparez puis relancez :

```bash
xattr -d com.apple.quarantine pdfium/libpdfium.dylib
codesign --force --sign - pdfium/libpdfium.dylib
codesign --verify pdfium/libpdfium.dylib
```

`./scripts/install-pdfium.sh` le fait automatiquement. Le message d'erreur de
YouPDF indique le chemin localisé et le motif exact du refus.

## Sans pdfium

Au démarrage, YouPDF affiche dans les logs :

```
renderer : pdfium
```

Si vous voyez à la place :

```
pdfium indisponible, rendu factice activé
```

alors la bibliothèque n'est pas au bon endroit (ou elle est refusée par
macOS : voir la section « quarantine » ci-dessus — le message d'erreur
détaille le chemin essayé et la cause). Le caviardage et le PDF
consultable nécessitent aussi pdfium (ils rendent la page pour la
rasteriser) et échouent avec un message clair dans ce cas. Le reste de
l'application reste utilisable (navigation, onglets, zoom) mais les pages
s'affichent en gris uni.
